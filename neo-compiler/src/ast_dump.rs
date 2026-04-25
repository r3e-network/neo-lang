//! Tree-style visualization of `syntax::ast::SourceFile` for CLI `ast` command.

use std::io;

use crate::syntax::ast::*;

pub(crate) struct AstDump<'a, Write: io::Write> {
    w: &'a mut Write,
}

impl<'a, Write: io::Write> AstDump<'a, Write> {
    pub(crate) fn new(w: &'a mut Write) -> Self {
        Self { w }
    }

    pub(crate) fn dump_source_file(&mut self, sf: &SourceFile) -> io::Result<()> {
        writeln!(self.w, "SourceFile")?;
        let anc: &[bool] = &[];

        let n = (sf.package.is_some() as usize)
            + sf.imports.len()
            + sf.structs.len()
            + sf.functions.len()
            + (sf.contract.is_some() as usize);

        let mut index = 0usize;
        if let Some(ref p) = sf.package {
            index += 1;
            self.tree_line(anc, index == n, &format!("package: {p:?}"))?;
        }

        for imp in &sf.imports {
            index += 1;
            let last = index == n;
            self.tree_line(anc, last, "ImportDecl")?;
            let anc = extend_ancestors(anc, last);
            self.tree_line(&anc, false, &format!("name: {}", imp.name))?;
            self.tree_line(&anc, true, &format!("library: {:?}", imp.library))?;
        }

        for s in &sf.structs {
            index += 1;
            self.dump_struct_decl(&anc, index == n, s)?;
        }

        for f in &sf.functions {
            index += 1;
            self.dump_function_decl(&anc, index == n, f)?;
        }

        if let Some(ref c) = sf.contract {
            index += 1;
            self.dump_contract(anc, index == n, c)?;
        }

        Ok(())
    }

    fn tree_line(&mut self, ancestors: &[bool], is_last: bool, label: &str) -> io::Result<()> {
        for &anc in ancestors {
            write!(self.w, "{}", if anc { "    " } else { "│   " })?;
        }
        write!(self.w, "{}", if is_last { "└── " } else { "├── " })?;
        writeln!(self.w, "{label}")?;
        Ok(())
    }

    fn dump_type(&mut self, ancestors: &[bool], is_last: bool, ty: &Type) -> io::Result<()> {
        self.tree_line(ancestors, is_last, &ty.to_string())?;
        Ok(())
    }

    fn dump_attribute(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        attr: &Attribute,
    ) -> io::Result<()> {
        let label = if attr.args.is_empty() {
            format!("#[{}]", attr.name)
        } else {
            let args = attr
                .args
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("#[{}({})]", attr.name, args)
        };
        self.tree_line(ancestors, is_last, &label)?;
        Ok(())
    }

    fn dump_param(&mut self, ancestors: &[bool], is_last: bool, p: &Param) -> io::Result<()> {
        self.tree_line(ancestors, is_last, "Param")?;
        let anc = extend_ancestors(ancestors, is_last);
        self.dump_type(&anc, false, &p.ty)?;
        self.tree_line(&anc, true, &format!("name: {}", p.name))?;
        Ok(())
    }

    fn dump_param_list(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        params: &[Param],
        label: &str,
    ) -> io::Result<()> {
        if params.is_empty() {
            self.tree_line(ancestors, is_last, &format!("{label} []"))?;
            return Ok(());
        }
        self.tree_line(ancestors, is_last, label)?;
        let anc = extend_ancestors(ancestors, is_last);
        let m = params.len();
        for (j, p) in params.iter().enumerate() {
            self.dump_param(&anc, j + 1 == m, p)?;
        }
        Ok(())
    }

    fn dump_block(&mut self, ancestors: &[bool], is_last: bool, block: &Block) -> io::Result<()> {
        self.tree_line(ancestors, is_last, "Block")?;
        let anc = extend_ancestors(ancestors, is_last);
        self.dump_inner_stmts(&anc, &block.stmts)
    }

    fn dump_inner_stmts(&mut self, ancestors: &[bool], stmts: &[Stmt]) -> io::Result<()> {
        let m = stmts.len();
        if m == 0 {
            self.tree_line(ancestors, true, "(empty)")?;
            return Ok(());
        }
        for (j, s) in stmts.iter().enumerate() {
            self.dump_stmt(ancestors, j + 1 == m, s)?;
        }
        Ok(())
    }

    fn dump_stmt(&mut self, ancestors: &[bool], is_last: bool, stmt: &Stmt) -> io::Result<()> {
        match stmt {
            Stmt::Var { name, init } => {
                self.tree_line(ancestors, is_last, "Stmt::Var")?;
                let anc = extend_ancestors(ancestors, is_last);
                match init {
                    None => self.tree_line(&anc, true, &format!("name: {name}"))?,
                    Some(expr) => {
                        self.tree_line(&anc, false, &format!("name: {name}"))?;
                        self.dump_expr(&anc, true, "init", expr)?;
                    }
                }
            }
            Stmt::Expr(expr) => {
                self.tree_line(ancestors, is_last, "Stmt::Expr")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, true, "expr", expr)?;
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.tree_line(ancestors, is_last, "Stmt::If")?;
                let anc = extend_ancestors(ancestors, is_last);
                if let Some(else_block) = else_block {
                    self.dump_expr(&anc, false, "cond", cond)?;
                    self.dump_block(&anc, false, then_block)?;
                    self.dump_block(&anc, true, else_block)?;
                } else {
                    self.dump_expr(&anc, false, "cond", cond)?;
                    self.dump_block(&anc, true, then_block)?;
                }
            }
            Stmt::ForArray { item, iter, body } => {
                self.tree_line(ancestors, is_last, "Stmt::ForArray")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.tree_line(&anc, false, &format!("item: {item}"))?;
                self.dump_expr(&anc, false, "iter", iter)?;
                self.dump_block(&anc, true, body)?;
            }
            Stmt::ForMap {
                key,
                value,
                map,
                body,
            } => {
                self.tree_line(ancestors, is_last, "Stmt::ForMap")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.tree_line(&anc, false, &format!("key: {key}, value: {value}"))?;
                self.dump_expr(&anc, false, "map", map)?;
                self.dump_block(&anc, true, body)?;
            }
            Stmt::While { cond, body } => {
                self.tree_line(ancestors, is_last, "Stmt::While")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, false, "cond", cond)?;
                self.dump_block(&anc, true, body)?;
            }
            Stmt::Return(expr) => {
                self.tree_line(ancestors, is_last, "Stmt::Return")?;
                let anc = extend_ancestors(ancestors, is_last);
                match expr {
                    None => self.tree_line(&anc, true, "(void)")?,
                    Some(ex) => self.dump_expr(&anc, true, "value", ex)?,
                }
            }
            Stmt::Emit { name, args } => {
                self.tree_line(ancestors, is_last, "Stmt::Emit")?;
                let anc = extend_ancestors(ancestors, is_last);
                if args.is_empty() {
                    self.tree_line(&anc, true, &format!("event: {name}"))?;
                } else {
                    self.tree_line(&anc, false, &format!("event: {name}"))?;
                    let args_size = args.len();
                    for (i, arg) in args.iter().enumerate() {
                        self.dump_expr(&anc, i + 1 == args_size, &format!("arg[{i}]"), arg)?;
                    }
                }
            }
            Stmt::Block(block) => {
                self.tree_line(ancestors, is_last, "Stmt::Block")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_inner_stmts(&anc, &block.stmts)?;
            }
        }
        Ok(())
    }

    fn dump_expr(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        label: &str,
        expr: &Expr,
    ) -> io::Result<()> {
        match expr {
            Expr::Literal(lit) => {
                self.tree_line(
                    ancestors,
                    is_last,
                    &format!("{label}: Literal {}", lit.to_string()),
                )?;
            }
            Expr::Ident(name) => {
                self.tree_line(ancestors, is_last, &format!("{label}: Ident `{name}`"))?;
            }
            Expr::Self_ => {
                self.tree_line(ancestors, is_last, &format!("{label}: self"))?;
            }
            Expr::Binary { op, left, right } => {
                self.tree_line(
                    ancestors,
                    is_last,
                    &format!("{label}: Binary `{}`", op.to_string()),
                )?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, false, "left", left)?;
                self.dump_expr(&anc, true, "right", right)?;
            }
            Expr::Unary { op, expr } => {
                self.tree_line(
                    ancestors,
                    is_last,
                    &format!("{label}: Unary `{}`", op.to_string()),
                )?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, true, "expr", expr)?;
            }
            Expr::Assign { target, op, value } => {
                self.tree_line(
                    ancestors,
                    is_last,
                    &format!("{label}: Assign `{}`", op.to_string()),
                )?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, false, "target", target)?;
                self.dump_expr(&anc, true, "value", value)?;
            }
            Expr::Cast { expr, ty } => {
                self.tree_line(
                    ancestors,
                    is_last,
                    &format!("{label}: Cast `as {}`", ty.to_string()),
                )?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, true, "expr", expr)?;
            }
            Expr::Member { base, field } => {
                self.tree_line(ancestors, is_last, &format!("{label}: Member `.{field}`"))?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, true, "base", base)?;
            }
            Expr::Index { base, index } => {
                self.tree_line(ancestors, is_last, &format!("{label}: Index"))?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, false, "base", base)?;
                self.dump_expr(&anc, true, "index", index)?;
            }
            Expr::Call { callee, args } => {
                self.tree_line(ancestors, is_last, &format!("{label}: Call"))?;
                let anc = extend_ancestors(ancestors, is_last);
                if args.is_empty() {
                    self.dump_expr(&anc, true, "callee", callee)?;
                } else {
                    self.dump_expr(&anc, false, "callee", callee)?;
                    let m = args.len();
                    for (j, arg) in args.iter().enumerate() {
                        self.dump_expr(&anc, j + 1 == m, &format!("arg[{j}]"), arg)?;
                    }
                }
            }
            Expr::StructLit { name, fields } => {
                self.tree_line(ancestors, is_last, &format!("{label}: StructLit `{name}`"))?;
                let anc = extend_ancestors(ancestors, is_last);
                let m = fields.len();
                if m == 0 {
                    self.tree_line(&anc, true, "(no fields)")?;
                } else {
                    for (j, (fname, ex)) in fields.iter().enumerate() {
                        let flast = j + 1 == m;
                        self.tree_line(&anc, flast, &format!("field `{fname}`"))?;
                        let anc2 = extend_ancestors(&anc, flast);
                        self.dump_expr(&anc2, true, "value", ex)?;
                    }
                }
            }
            Expr::MapLit { ty, pairs } => {
                self.tree_line(ancestors, is_last, &format!("{label}: MapLit ({ty:?})"))?;
                let anc = extend_ancestors(ancestors, is_last);
                let m = pairs.len();
                if m == 0 {
                    self.tree_line(&anc, true, "(empty)")?;
                } else {
                    for (j, (k, v)) in pairs.iter().enumerate() {
                        let plast = j + 1 == m;
                        self.tree_line(&anc, plast, &format!("pair[{j}]"))?;
                        let anc2 = extend_ancestors(&anc, plast);
                        self.dump_expr(&anc2, false, "key", k)?;
                        self.dump_expr(&anc2, true, "value", v)?;
                    }
                }
            }
            Expr::ArrayLit { ty, elements } => {
                self.tree_line(ancestors, is_last, &format!("{label}: ArrayLit ({ty:?})"))?;
                let anc = extend_ancestors(ancestors, is_last);
                let m = elements.len();
                if m == 0 {
                    self.tree_line(&anc, true, "(empty)")?;
                } else {
                    for (j, item) in elements.iter().enumerate() {
                        self.dump_expr(&anc, j + 1 == m, &format!("[{j}]"), item)?;
                    }
                }
            }
            Expr::Paren(inner) => {
                self.tree_line(ancestors, is_last, &format!("{label}: Paren"))?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_expr(&anc, true, "inner", inner)?;
            }
        }
        Ok(())
    }

    fn dump_struct_decl(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        decel: &StructDecl,
    ) -> io::Result<()> {
        self.tree_line(ancestors, is_last, &format!("StructDecl `{}`", decel.name))?;
        let anc = extend_ancestors(ancestors, is_last);
        let nf = decel.fields.len();
        let nm = decel.methods.len();
        if nf == 0 && nm == 0 {
            self.tree_line(&anc, true, "(no fields or methods)")?;
            return Ok(());
        }
        let total = nf + nm;
        let mut k = 0usize;
        for f in &decel.fields {
            k += 1;
            self.dump_struct_field(&anc, k == total, f)?;
        }
        for m in &decel.methods {
            k += 1;
            self.dump_function_decl(&anc, k == total, m)?;
        }
        Ok(())
    }

    fn dump_struct_field(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        field: &StructField,
    ) -> io::Result<()> {
        self.tree_line(ancestors, is_last, &format!("StructField `{}`", field.name))?;
        let anc = extend_ancestors(ancestors, is_last);
        match &field.init {
            None => {
                self.dump_type(&anc, false, &field.ty)?;
                self.tree_line(&anc, true, &format!("name: {}", field.name))?;
            }
            Some(init) => {
                self.dump_type(&anc, false, &field.ty)?;
                self.tree_line(&anc, false, &format!("name: {}", field.name))?;
                self.dump_expr(&anc, true, "init", init)?;
            }
        }
        Ok(())
    }

    fn dump_function_decl(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        f: &FunctionDecl,
    ) -> io::Result<()> {
        self.tree_line(ancestors, is_last, &format!("FunctionDecl `{}`", f.name))?;
        let anc = extend_ancestors(ancestors, is_last);
        let total = 3 + (!f.attributes.is_empty() as usize);
        let mut index = 0usize;
        if !f.attributes.is_empty() {
            index += 1;
            let sec_last = index == total;
            self.tree_line(&anc, sec_last, "attributes")?;
            let anc2 = extend_ancestors(&anc, sec_last);
            let m = f.attributes.len();
            for (j, attr) in f.attributes.iter().enumerate() {
                self.dump_attribute(&anc2, j + 1 == m, attr)?;
            }
        }

        index += 1;
        self.tree_line(
            &anc,
            index == total,
            &format!("return: {}", f.return_ty.to_string()),
        )?;

        index += 1;
        self.dump_param_list(&anc, index == total, &f.params, "params")?;

        index += 1;
        self.dump_block(&anc, index == total, &f.body)?;

        Ok(())
    }

    fn dump_contract(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        contract: &ContractDecl,
    ) -> io::Result<()> {
        self.tree_line(ancestors, is_last, &format!("Contract `{}`", contract.name))?;
        let anc = extend_ancestors(ancestors, is_last);
        let n = contract.attributes.len() + contract.members.len();
        if n == 0 {
            self.tree_line(&anc, true, "(empty)")?;
            return Ok(());
        }
        let mut index = 0usize;
        for attr in &contract.attributes {
            index += 1;
            self.dump_attribute(&anc, index == n, attr)?;
        }
        for mem in &contract.members {
            index += 1;
            self.dump_contract_member(&anc, index == n, mem)?;
        }
        Ok(())
    }

    fn dump_contract_member(
        &mut self,
        ancestors: &[bool],
        is_last: bool,
        member: &ContractMember,
    ) -> io::Result<()> {
        match member {
            ContractMember::ConstProp(const_prop) => {
                self.tree_line(ancestors, is_last, "ConstProp")?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_type(&anc, false, &const_prop.ty)?;
                self.tree_line(&anc, false, &format!("name: {}", const_prop.name))?;
                self.dump_expr(&anc, true, "init", &const_prop.init)?;
            }
            ContractMember::Field(field) => {
                self.tree_line(ancestors, is_last, "ContractField")?;
                let anc = extend_ancestors(ancestors, is_last);
                match &field.init {
                    None => {
                        self.dump_type(&anc, false, &field.ty)?;
                        self.tree_line(&anc, true, &format!("name: {}", field.name))?;
                    }
                    Some(init) => {
                        self.dump_type(&anc, false, &field.ty)?;
                        self.tree_line(&anc, false, &format!("name: {}", field.name))?;
                        self.dump_expr(&anc, true, "init", init)?;
                    }
                }
            }
            ContractMember::Event(event) => {
                self.tree_line(ancestors, is_last, &format!("Event `{}`", event.name))?;
                let anc = extend_ancestors(ancestors, is_last);
                self.dump_param_list(&anc, true, &event.params, "params")?;
            }
            ContractMember::Function(func) => {
                self.dump_function_decl(ancestors, is_last, func)?;
            }
        }
        Ok(())
    }
}

fn extend_ancestors(ancestors: &[bool], is_last: bool) -> Vec<bool> {
    let mut v = ancestors.to_vec();
    v.push(is_last);
    v
}
