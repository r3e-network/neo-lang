//! Neo-lang compiler.

pub mod codegen;
pub mod ir;
pub mod syntax;
pub mod target;
pub mod typecheck;

mod asm_dump;
mod ast_dump;
mod build_target;
mod disasm;

use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::build_target::run_build;
use crate::codegen::Codegen;
use crate::target::nef::Nef3;

/// Maximum source file size (bytes) for `Ast` and related commands.
pub(crate) const MAX_SOURCE_FILE_BYTES: u64 = 256 * 1024;

/// Maximum binary size (bytes) for `disasm` (NEF or raw script).
pub(crate) const MAX_DEASM_INPUT_BYTES: u64 = 4 * 1024 * 1024;

/// Maximum manifest JSON size for `disasm --manifest`.
pub(crate) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Parser)]
#[command(name = "neo-compiler", version, about = "neo-lang compiler")]
#[command(subcommand_required = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a source file and print its AST as a tree
    Ast {
        /// Path to a .neo source file
        file: std::path::PathBuf,
    },
    /// Compile a source file and print NeoVM instructions (disassembly-style listing)
    Asm {
        /// Path to a .neo source file
        file: std::path::PathBuf,
    },
    /// Compile a source file to NEF + manifest
    Build {
        /// Path to a .neo source file
        source: std::path::PathBuf,
    },
    /// Disassemble a NEF3 (`.nef`) file or raw VM bytecode to stdout (same listing style as `asm`)
    Disasm {
        /// Path to a `.nef` file, or a file containing raw NeoVM script bytes
        file: std::path::PathBuf,
        /// Optional contract manifest (`.manifest.json`) for function section headers (`Contract::method`)
        #[arg(long)]
        manifest: Option<std::path::PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Ast { file } => match run_ast(&file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(io::stderr(), "Running `ast` command error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Asm { file } => match run_asm(&file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(io::stderr(), "Running `asm` command error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Build { source } => match run_build(&source) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(io::stderr(), "Running `build` command error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Disasm { file, manifest } => match run_disasm(&file, manifest.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(io::stderr(), "Running `disasm` command error: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_ast(path: &std::path::Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "{}: file too large (max {} KiB)",
            path.display(),
            MAX_SOURCE_FILE_BYTES / 1024
        ));
    }
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ast = syntax::parser::parse_source_file(&src)
        .map_err(|e| format!("parse error at token line {}: {}", e.line, e.message))?;
    let mut out = io::stdout().lock();
    let mut ast_dump = ast_dump::AstDump::new(&mut out);
    ast_dump.dump_source_file(&ast).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_asm(path: &std::path::Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "{}: file too large (max {} KiB)",
            path.display(),
            MAX_SOURCE_FILE_BYTES / 1024
        ));
    }
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ast = syntax::parser::parse_source_file(&src)
        .map_err(|e| format!("parse error at token line {}: {}", e.line, e.message))?;
    let compiled = Codegen::new()
        .codegen_source_file(&ast)
        .map_err(|e| e.to_string())?;
    let mut out = io::stdout().lock();
    asm_dump::AsmDump::new(&mut out)
        .dump_compiled_source(&compiled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn run_disasm(
    path: &std::path::Path,
    manifest_path: Option<&std::path::Path>,
) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_DEASM_INPUT_BYTES {
        return Err(format!(
            "{}: file too large (max {} MiB)",
            path.display(),
            MAX_DEASM_INPUT_BYTES / (1024 * 1024)
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let is_nef = bytes.len() >= 4
        && u32::from_le_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| "disasm: file too small")?,
        ) == Nef3::MAGIC;
    let script = if is_nef {
        Nef3::extract_script(&bytes).map_err(|e| format!("disasm: extract script error: {e}"))?
    } else {
        bytes
    };
    let instructions =
        disasm::decode_script(&script).map_err(|e| format!("disasm: decode script error: {e}"))?;

    let manifest = if let Some(mp) = manifest_path {
        let mm = fs::metadata(mp).map_err(|e| format!("{}: {e}", mp.display()))?;
        if mm.len() > MAX_MANIFEST_BYTES {
            return Err(format!(
                "{}: manifest too large (max {} KiB)",
                mp.display(),
                MAX_MANIFEST_BYTES / 1024
            ));
        }
        let s = fs::read_to_string(mp)
            .map_err(|e| format!("disasm: read manifest {} error: {e}", mp.display()))?;
        Some(
            disasm::parse_manifest_json(&s)
                .map_err(|e| format!("disasm: parse manifest {} error: {e}", mp.display()))?,
        )
    } else {
        None
    };

    let title = if is_nef { "NEF script" } else { "Raw script" };
    let mut out = io::stdout().lock();
    disasm::write_disassembly_listing(&mut out, title, &instructions, manifest.as_ref())
        .map_err(|e| format!("disasm: write listing error: {e}"))?;
    Ok(())
}
