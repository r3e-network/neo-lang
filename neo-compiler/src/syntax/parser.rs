//! Hand-written recursive-descent + Pratt parser (avoids LALRPOP codegen freeze).

use crate::syntax::ast::*;
use crate::syntax::lexer::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
}

pub fn parse_source_file(src: &str) -> Result<SourceFile, ParseError> {
    let tokens = lex(src).map_err(|err| ParseError {
        message: err.message.into(),
        line: err.line,
    })?;
    let mut parser = Parser { tokens, index: 0 };
    parser.parse_source_file()
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    index: usize,
}

impl Parser {
    fn current(&self) -> &Token {
        self.tokens
            .get(self.index)
            .map(|(_, token)| token)
            .unwrap_or(&self.tokens.last().unwrap().1)
    }

    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            line: self
                .tokens
                .get(self.index)
                .map(|(line, _)| line)
                .copied()
                .unwrap_or(self.tokens.last().unwrap().0),
        }
    }

    fn expect(&mut self, want: &str, pred: impl FnOnce(&Token) -> bool) -> Result<(), ParseError> {
        if pred(self.current()) {
            self.bump();
            Ok(())
        } else {
            Err(self.err(format!("expected {want}, got {:?}", self.current())))
        }
    }

    fn eat_ident(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Token::Ident(ident) => Ok(ident),
            token => Err(self.err(format!("expected identifier, got {token:?}"))),
        }
    }

    fn parse_source_file(&mut self) -> Result<SourceFile, ParseError> {
        let mut package = None;
        if matches!(self.current(), Token::Package) {
            self.bump();
            package = Some(self.eat_ident()?);
            self.expect("';'", |t| matches!(t, Token::Semi))?;
        }
        let mut imports = Vec::new();
        while matches!(self.current(), Token::Import) {
            imports.push(self.parse_import()?);
        }

        let mut source = SourceFile {
            package,
            imports,
            structs: Vec::new(),
            functions: Vec::new(),
            contract: None,
        };
        while !matches!(self.current(), Token::Eof) {
            let attrs = self.parse_attributes_opt()?;
            if matches!(self.current(), Token::Struct) {
                self.bump();
                source.structs.push(self.parse_struct_decl()?);
            } else if matches!(self.current(), Token::Contract) {
                if source.contract.is_some() {
                    return Err(self.err("only one contract is allowed"));
                }

                self.bump();
                let name = self.eat_ident()?;
                self.expect("'{'", |t| matches!(t, Token::LBrace))?;
                let mut members = Vec::new();
                while !matches!(self.current(), Token::RBrace) {
                    members.push(self.parse_contract_member()?);
                }
                self.expect("'}'", |t| matches!(t, Token::RBrace))?;
                source.contract = Some(ContractDecl {
                    attributes: attrs,
                    name,
                    members,
                });
            } else {
                source.functions.push(self.parse_function_decl_rest(attrs)?);
            }
        }
        Ok(source)
    }

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        self.expect("'import'", |token| matches!(token, Token::Import))?;
        let name = self.eat_ident()?;
        self.expect("'from'", |token| matches!(token, Token::From))?;
        let library = match self.bump() {
            Token::StringLit(s) => s,
            token => return Err(self.err(format!("expected string after from, got {token:?}"))),
        };
        self.expect("';'", |token| matches!(token, Token::Semi))?;
        Ok(ImportDecl { name, library })
    }

    fn parse_struct_decl(&mut self) -> Result<StructDecl, ParseError> {
        let name = self.eat_ident()?;
        self.expect("'{'", |token| matches!(token, Token::LBrace))?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !matches!(self.current(), Token::RBrace) {
            let attrs = self.parse_attributes_opt()?;
            let ty = self.parse_type()?;
            let mem_name = self.eat_ident()?;
            if matches!(self.current(), Token::LParen) {
                self.bump();
                let params = self.parse_param_list()?;
                self.expect("')'", |token| matches!(token, Token::RParen))?;
                let body = self.parse_block()?;
                methods.push(FunctionDecl {
                    attributes: attrs,
                    return_ty: ty,
                    name: mem_name,
                    params,
                    body,
                });
            } else {
                if !attrs.is_empty() {
                    return Err(self.err("struct fields cannot have attributes"));
                }
                let init = if matches!(self.current(), Token::Eq) {
                    self.bump();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect("';'", |token| matches!(token, Token::Semi))?;
                fields.push(StructField {
                    ty,
                    name: mem_name,
                    init,
                });
            }
        }
        self.expect("'}'", |token| matches!(token, Token::RBrace))?;
        Ok(StructDecl {
            name,
            fields,
            methods,
        })
    }

    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        self.expect("'#[", |token| matches!(token, Token::AttrOpen))?;
        let name = self.eat_ident()?;
        let args = if matches!(self.current(), Token::LParen) {
            self.bump();
            let mut args: Vec<String> = Vec::new();
            if !matches!(self.current(), Token::RParen) {
                loop {
                    match self.bump() {
                        Token::StringLit(s) => args.push(s),
                        token => {
                            return Err(
                                self.err(format!("expected string in attribute, got {token:?}"))
                            )
                        }
                    }
                    if matches!(self.current(), Token::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            self.expect("')'", |token| matches!(token, Token::RParen))?;
            args
        } else {
            vec![]
        };
        self.expect("']'", |token| matches!(token, Token::RBracket))?;
        Ok(Attribute { name, args })
    }

    fn parse_contract_member(&mut self) -> Result<ContractMember, ParseError> {
        if matches!(self.current(), Token::Const) {
            self.bump();
            let ty = self.parse_type()?;
            let name = self.eat_ident()?;
            self.expect("'='", |token| matches!(token, Token::Eq))?;
            let init = self.parse_expr()?;
            self.expect("';'", |token| matches!(token, Token::Semi))?;
            return Ok(ContractMember::ConstProp(ConstProp { ty, name, init }));
        }
        if matches!(self.current(), Token::Event) {
            self.bump();
            let name = self.eat_ident()?;
            self.expect("'('", |token| matches!(token, Token::LParen))?;
            let params = self.parse_param_list()?;
            self.expect("')'", |token| matches!(token, Token::RParen))?;
            self.expect("';'", |token| matches!(token, Token::Semi))?;
            return Ok(ContractMember::Event(EventDecl { name, params }));
        }
        let attrs = self.parse_attributes_opt()?;
        let ty = self.parse_type()?;
        let name = self.eat_ident()?;
        if matches!(self.current(), Token::LParen) {
            self.bump();
            let params = self.parse_param_list()?;
            self.expect("')'", |token| matches!(token, Token::RParen))?;
            let body = self.parse_block()?;
            return Ok(ContractMember::Function(FunctionDecl {
                attributes: attrs,
                return_ty: ty,
                name,
                params,
                body,
            }));
        }
        let init = if matches!(self.current(), Token::Eq) {
            self.bump();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect("';'", |token| matches!(token, Token::Semi))?;
        Ok(ContractMember::Field(ContractField { ty, name, init }))
    }

    fn parse_attributes_opt(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attrs = Vec::new();
        while matches!(self.current(), Token::AttrOpen) {
            attrs.push(self.parse_attribute()?);
        }
        Ok(attrs)
    }

    fn parse_function_decl_rest(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<FunctionDecl, ParseError> {
        let return_ty = self.parse_type()?;
        let name = self.eat_ident()?;
        self.expect("'('", |token| matches!(token, Token::LParen))?;
        let params = self.parse_param_list()?;
        self.expect("')'", |token| matches!(token, Token::RParen))?;
        let body = self.parse_block()?;
        Ok(FunctionDecl {
            attributes,
            return_ty,
            name,
            params,
            body,
        })
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut ps = Vec::new();
        if matches!(self.current(), Token::RParen) {
            return Ok(ps);
        }
        loop {
            let ty = self.parse_type()?;
            let name = self.eat_ident()?;
            ps.push(Param { ty, name });
            if matches!(self.current(), Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(ps)
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect("'{'", |token| matches!(token, Token::LBrace))?;
        let mut stmts = Vec::new();
        while !matches!(self.current(), Token::RBrace) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect("'}'", |token| matches!(token, Token::RBrace))?;
        Ok(Block { stmts })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.current().clone() {
            Token::Var => {
                self.bump();
                let name = self.eat_ident()?;
                let init = if matches!(self.current(), Token::Eq) {
                    self.bump();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect("';'", |token| matches!(token, Token::Semi))?;
                Ok(Stmt::Var { name, init })
            }
            Token::Return => {
                self.bump();
                if matches!(self.current(), Token::Semi) {
                    self.bump();
                    Ok(Stmt::Return(None))
                } else {
                    let e = self.parse_expr()?;
                    self.expect("';'", |token| matches!(token, Token::Semi))?;
                    Ok(Stmt::Return(Some(e)))
                }
            }
            Token::If => {
                self.bump();
                let cond = self.parse_expr()?;
                let then_block = self.parse_block()?;
                let else_block = if matches!(self.current(), Token::Else) {
                    self.bump();
                    Some(self.parse_block()?)
                } else {
                    None
                };
                Ok(Stmt::If {
                    cond,
                    then_block,
                    else_block,
                })
            }
            Token::While => {
                self.bump();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, body })
            }
            Token::For => {
                self.bump();
                let ident = self.eat_ident()?;
                if matches!(self.current(), Token::Comma) {
                    self.bump();
                    let value_ident = self.eat_ident()?;
                    self.expect("'in'", |token| matches!(token, Token::In))?;
                    let map_expr = self.parse_expr()?;
                    let body = self.parse_block()?;
                    Ok(Stmt::ForMap {
                        key: ident,
                        value: value_ident,
                        map: map_expr,
                        body,
                    })
                } else {
                    self.expect("'in'", |token| matches!(token, Token::In))?;
                    let iter = self.parse_expr()?;
                    let body = self.parse_block()?;
                    Ok(Stmt::ForArray {
                        item: ident,
                        iter,
                        body,
                    })
                }
            }
            Token::Emit => {
                self.bump();
                let name = self.eat_ident()?;
                self.expect("'('", |token| matches!(token, Token::LParen))?;
                let args = self.parse_expr_list()?;
                self.expect("')'", |token| matches!(token, Token::RParen))?;
                self.expect("';'", |token| matches!(token, Token::Semi))?;
                Ok(Stmt::Emit { name, args })
            }
            Token::LBrace => Ok(Stmt::Block(self.parse_block()?)),
            _ => {
                let expr = self.parse_expr()?;
                self.expect("';'", |token| matches!(token, Token::Semi))?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut v = Vec::new();
        if matches!(self.current(), Token::RParen) {
            return Ok(v);
        }
        loop {
            v.push(self.parse_expr()?);
            if matches!(self.current(), Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_expr_bp(0)
    }

    /// Binding power: higher = tighter. Left-assoc: rhs uses `l + 1`. Right-assoc: rhs uses `l`.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        enum InfixOp {
            Binary(BinaryOp),
            Assign(AssignOp),
        }
        let mut lhs = self.parse_operand()?;
        loop {
            let (l_bp, r_bp, infix_op) = match self.current() {
                Token::PipePipe => (20, 21, InfixOp::Binary(BinaryOp::Or)),
                Token::AmpAmp => (30, 31, InfixOp::Binary(BinaryOp::And)),
                Token::EqEq => (40, 41, InfixOp::Binary(BinaryOp::Eq)),
                Token::Ne => (40, 41, InfixOp::Binary(BinaryOp::Ne)),
                Token::Lt => (40, 41, InfixOp::Binary(BinaryOp::Lt)),
                Token::Le => (40, 41, InfixOp::Binary(BinaryOp::Le)),
                Token::Gt => (40, 41, InfixOp::Binary(BinaryOp::Gt)),
                Token::Ge => (40, 41, InfixOp::Binary(BinaryOp::Ge)),
                Token::Caret => (50, 51, InfixOp::Binary(BinaryOp::BitXor)),
                Token::Pipe => (60, 61, InfixOp::Binary(BinaryOp::BitOr)),
                Token::Amp => (70, 71, InfixOp::Binary(BinaryOp::BitAnd)),
                Token::Shl => (80, 81, InfixOp::Binary(BinaryOp::Shl)),
                Token::Shr => (80, 81, InfixOp::Binary(BinaryOp::Shr)),
                Token::Plus => (90, 91, InfixOp::Binary(BinaryOp::Add)),
                Token::Minus => (90, 91, InfixOp::Binary(BinaryOp::Sub)),
                Token::Star => (100, 101, InfixOp::Binary(BinaryOp::Mul)),
                Token::Slash => (100, 101, InfixOp::Binary(BinaryOp::Div)),
                Token::Percent => (100, 101, InfixOp::Binary(BinaryOp::Mod)),
                Token::Eq => (10, 10, InfixOp::Assign(AssignOp::Assign)),
                Token::PlusEq => (10, 10, InfixOp::Assign(AssignOp::PlusAssign)),
                Token::MinusEq => (10, 10, InfixOp::Assign(AssignOp::MinusAssign)),
                Token::StarEq => (10, 10, InfixOp::Assign(AssignOp::StarAssign)),
                Token::SlashEq => (10, 10, InfixOp::Assign(AssignOp::SlashAssign)),
                Token::PercentEq => (10, 10, InfixOp::Assign(AssignOp::PercentAssign)),
                Token::ShrEq => (10, 10, InfixOp::Assign(AssignOp::ShrAssign)),
                Token::ShlEq => (10, 10, InfixOp::Assign(AssignOp::ShlAssign)),
                Token::AmpEq => (10, 10, InfixOp::Assign(AssignOp::AmpAssign)),
                Token::PipeEq => (10, 10, InfixOp::Assign(AssignOp::PipeAssign)),
                Token::CaretEq => (10, 10, InfixOp::Assign(AssignOp::CaretAssign)),
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr_bp(r_bp)?;
            lhs = match infix_op {
                InfixOp::Binary(op) => Expr::Binary {
                    op,
                    left: Box::new(lhs),
                    right: Box::new(rhs),
                },
                InfixOp::Assign(op) => Expr::Assign {
                    target: Box::new(lhs),
                    op,
                    value: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn parse_operand(&mut self) -> Result<Expr, ParseError> {
        let mut unaries = Vec::new();
        loop {
            match self.current() {
                Token::Plus => {
                    self.bump();
                    unaries.push(UnaryOp::Positive);
                }
                Token::Minus => {
                    self.bump();
                    unaries.push(UnaryOp::Negative);
                }
                Token::Bang => {
                    self.bump();
                    unaries.push(UnaryOp::Not);
                }
                Token::Tilde => {
                    self.bump();
                    unaries.push(UnaryOp::BitNot);
                }
                _ => break,
            }
        }
        let mut expr = self.parse_primary()?;
        expr = self.parse_postfix_chain(expr)?;
        for unary in unaries.into_iter().rev() {
            expr = Expr::Unary {
                op: unary,
                expr: Box::new(expr),
            };
        }
        while matches!(self.current(), Token::As) {
            self.bump();
            let ty = self.parse_type()?;
            expr = Expr::Cast {
                expr: Box::new(expr),
                ty,
            };
        }
        Ok(expr)
    }

    fn parse_postfix_chain(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            match self.current().clone() {
                Token::Dot => {
                    self.bump();
                    let field = self.eat_ident()?;
                    expr = Expr::Member {
                        base: Box::new(expr),
                        field: field,
                    };
                }
                Token::LBracket => {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect("']'", |token| matches!(token, Token::RBracket))?;
                    expr = Expr::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Token::LParen => {
                    self.bump();
                    let args = self.parse_expr_list()?;
                    self.expect("')'", |token| matches!(token, Token::RParen))?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                Token::LBrace => {
                    // Only PascalCase names: avoids `for x in arr { }` parsing as struct `arr { }`.
                    let name = if let Expr::Ident(n) = &expr {
                        if n.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) {
                            n.clone()
                        } else {
                            break;
                        }
                    } else {
                        break;
                    };
                    self.bump();
                    let fields = self.parse_struct_field_inits()?;
                    self.expect("'}'", |token| matches!(token, Token::RBrace))?;
                    expr = Expr::StructLit { name, fields };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_struct_field_inits(&mut self) -> Result<Vec<(String, Expr)>, ParseError> {
        let mut v = Vec::new();
        if matches!(self.current(), Token::RBrace) {
            return Ok(v);
        }
        loop {
            let n = self.eat_ident()?;
            self.expect("':'", |token| matches!(token, Token::Colon))?;
            let ex = self.parse_expr()?;
            v.push((n, ex));
            if matches!(self.current(), Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        if let Some(expr) = self.try_parse_type_then_brace_literal()? {
            return Ok(expr);
        }
        match self.bump() {
            Token::Null => Ok(Expr::Literal(Literal::Null)),
            Token::True => Ok(Expr::Literal(Literal::Bool(true))),
            Token::False => Ok(Expr::Literal(Literal::Bool(false))),
            Token::Self_ => Ok(Expr::Self_),
            Token::IntLit(raw) => Ok(Expr::Literal(Literal::Int(raw))),
            Token::StringLit(s) => Ok(Expr::Literal(Literal::String(s))),
            Token::BufferLit(s) => Ok(Expr::Literal(Literal::Buffer(s))),
            Token::Ident(name) => Ok(Expr::Ident(name)),
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect("')'", |token| matches!(token, Token::RParen))?;
                Ok(Expr::Paren(Box::new(inner)))
            }
            Token::LBracket => {
                Err(self.err("array literal must specify element type: use ElemType[] { ... }"))
            }
            Token::LBrace => Err(self.err(
                "map literal must specify key and value types: use map<KeyType, ValueType> { ... }",
            )),
            token => Err(self.err(format!("unexpected token in expression: {token:?}"))),
        }
    }

    /// `map[K,V] { ... }` or `T[] { ... }` (and `T[][] { ... }`, …): type, then `{` body.
    fn try_parse_type_then_brace_literal(&mut self) -> Result<Option<Expr>, ParseError> {
        if !Self::token_starts_type(self.current()) {
            return Ok(None);
        }
        let checkpoint = self.index;
        let ty = match self.parse_type() {
            Ok(ty) => ty,
            Err(_) => {
                self.index = checkpoint;
                return Ok(None);
            }
        };
        if !matches!(self.current(), Token::LBrace) {
            self.index = checkpoint;
            return Ok(None);
        }
        self.bump(); // `{`
        let expr = match &ty {
            Type::Map { .. } => {
                let pairs = self.parse_map_literal_contents()?;
                self.expect("'}'", |token| matches!(token, Token::RBrace))?;
                Expr::MapLit { ty, pairs }
            }
            Type::Array(_) => {
                let elements = self.parse_array_literal_contents()?;
                self.expect("'}'", |token| matches!(token, Token::RBrace))?;
                Expr::ArrayLit { ty, elements }
            }
            _ => {
                self.index = checkpoint;
                return Ok(None);
            }
        };
        Ok(Some(expr))
    }

    fn token_starts_type(token: &Token) -> bool {
        matches!(
            token,
            Token::Void
                | Token::Bool
                | Token::Int
                | Token::String
                | Token::Hash160
                | Token::Hash256
                | Token::Buffer
                | Token::Any
                | Token::Map
                | Token::Ident(_)
        )
    }

    fn parse_array_literal_contents(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut exprs = Vec::new();
        if matches!(self.current(), Token::RBrace) {
            return Ok(exprs);
        }
        loop {
            exprs.push(self.parse_expr()?);
            if matches!(self.current(), Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(exprs)
    }

    /// After `{` of a map literal: parse `key: value` entries until `}` (exclusive).
    fn parse_map_literal_contents(&mut self) -> Result<Vec<(Expr, Expr)>, ParseError> {
        let mut pairs = Vec::new();
        if matches!(self.current(), Token::RBrace) {
            return Ok(pairs);
        }
        loop {
            let key = self.parse_expr()?;
            self.expect("':'", |t| matches!(t, Token::Colon))?;
            let value = self.parse_expr()?;
            pairs.push((key, value));
            if matches!(self.current(), Token::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(pairs)
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let mut ty = self.parse_type_base()?;
        loop {
            if !matches!(self.current(), Token::LBracket) {
                break;
            }
            // Only `T[]` is an array type suffix; `id[index]` must stay as indexing, not type parse.
            let empty = self
                .tokens
                .get(self.index + 1)
                .is_some_and(|(_, tok)| matches!(tok, Token::RBracket));
            if !empty {
                break;
            }
            self.bump(); // `[`
            self.bump(); // `]`
            ty = Type::Array(Box::new(ty));
        }
        Ok(ty)
    }

    fn parse_type_base(&mut self) -> Result<Type, ParseError> {
        match self.bump() {
            Token::Void => Ok(Type::Void),
            Token::Bool => Ok(Type::Bool),
            Token::Int => Ok(Type::Int),
            Token::String => Ok(Type::String),
            Token::Hash160 => Ok(Type::Hash160),
            Token::Hash256 => Ok(Type::Hash256),
            Token::Buffer => Ok(Type::Buffer),
            Token::Any => Ok(Type::Any),
            Token::Map => {
                self.expect("'['", |token| matches!(token, Token::LBracket))?;
                let key = self.parse_type()?;
                self.expect("','", |token| matches!(token, Token::Comma))?;
                let value = self.parse_type()?;
                self.expect("']'", |token| matches!(token, Token::RBracket))?;
                Ok(Type::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                })
            }
            Token::Ident(n) => Ok(Type::Named(n)),
            token => Err(self.err(format!("expected type, got {token:?}"))),
        }
    }
}
