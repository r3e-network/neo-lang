//! Hand-written recursive-descent + Pratt parser (avoids LALRPOP codegen freeze).

use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::ast::*;
use crate::syntax::lexer::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub span: Span,
}

pub fn parse_source_file(src: &str) -> Result<SourceFile, ParseError> {
    let lexed = lex(src).map_err(ParseError::from)?;
    let mut parser = Parser {
        tokens: lexed.tokens,
        spans: lexed.spans,
        index: 0,
        expr_event_stack: Vec::new(),
    };
    parser.parse_source_file()
}

pub fn parse_decl_file(src: &str) -> Result<DeclFile, ParseError> {
    let lexed = lex(src).map_err(ParseError::from)?;
    let mut parser = Parser {
        tokens: lexed.tokens,
        spans: lexed.spans,
        index: 0,
        expr_event_stack: Vec::new(),
    };
    parser.parse_decl_file()
}

pub fn parse_struct_file(src: &str) -> Result<Vec<StructDecl>, ParseError> {
    let lexed = lex(src).map_err(ParseError::from)?;
    let mut parser = Parser {
        tokens: lexed.tokens,
        spans: lexed.spans,
        index: 0,
        expr_event_stack: Vec::new(),
    };
    parser.parse_struct_file()
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    spans: Vec<Span>,
    index: usize,
    expr_event_stack: Vec<Vec<Span>>,
}

impl From<LexError> for ParseError {
    fn from(value: LexError) -> Self {
        Self {
            message: value.message,
            line: value.line,
            span: value.span,
        }
    }
}

impl ParseError {
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::error(&self.message, Some(self.span)).with_label(&self.message)
    }
}

impl Parser {
    fn current(&self) -> &Token {
        self.tokens
            .get(self.index)
            .map(|(_, token)| token)
            .unwrap_or(&self.tokens.last().unwrap().1)
    }

    fn current_span(&self) -> Span {
        self.spans
            .get(self.index)
            .copied()
            .or_else(|| self.spans.last().copied())
            .unwrap_or_default()
    }

    fn span_from(&self, start_index: usize) -> Span {
        let end_index = self.index.saturating_sub(1);
        if start_index <= end_index {
            Span::new(self.spans[start_index].start, self.spans[end_index].end)
        } else {
            self.current_span()
        }
    }

    fn push_expr_span(&mut self, span: Span) {
        if let Some(events) = self.expr_event_stack.last_mut() {
            events.push(span);
        }
    }

    fn pop_expr_span(&mut self) {
        if let Some(events) = self.expr_event_stack.last_mut() {
            events.pop();
        }
    }

    fn finish_block(&mut self, mut block: Block) -> Block {
        let events = self.expr_event_stack.pop().unwrap_or_default();
        block.expr_spans = build_expr_span_map(&block, events);
        block
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
                .map(|(line, _)| *line)
                .unwrap_or(self.tokens.last().map(|(line, _)| *line).unwrap_or(1)),
            span: self.current_span(),
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
            struct_spans: Vec::new(),
            function_spans: Vec::new(),
            contract_member_spans: Vec::new(),
        };
        while !matches!(self.current(), Token::Eof) {
            let item_start = self.index;
            let attrs = self.parse_attributes_opt()?;
            if matches!(self.current(), Token::Struct) {
                self.bump();
                source.structs.push(self.parse_struct_decl()?);
                source.struct_spans.push(self.span_from(item_start));
            } else if matches!(self.current(), Token::Contract) {
                if source.contract.is_some() {
                    return Err(self.err("only one contract is allowed"));
                }

                self.bump();
                let name = self.eat_ident()?;
                self.expect("'{'", |t| matches!(t, Token::LBrace))?;
                let mut members = Vec::new();
                let mut member_spans = Vec::new();
                while !matches!(self.current(), Token::RBrace) {
                    let member_start = self.index;
                    members.push(self.parse_contract_member()?);
                    member_spans.push(self.span_from(member_start));
                }
                self.expect("'}'", |t| matches!(t, Token::RBrace))?;
                source.contract = Some(ContractDecl {
                    attributes: attrs,
                    name,
                    members,
                });
                source.contract_member_spans = member_spans;
            } else {
                source.functions.push(self.parse_function_decl_rest(attrs)?);
                source.function_spans.push(self.span_from(item_start));
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

    fn parse_decl_file(&mut self) -> Result<DeclFile, ParseError> {
        let mut structs = Vec::new();
        while matches!(self.current(), Token::Struct) {
            self.bump();
            structs.push(self.parse_struct_decl()?);
        }
        let mut contracts = Vec::new();
        while !matches!(self.current(), Token::Eof) {
            contracts.push(self.parse_extern_contract_decl()?);
        }
        if structs.is_empty() && contracts.is_empty() {
            return Err(self.err("`.d.neo` file must declare at least one struct or contract"));
        }
        Ok(DeclFile { structs, contracts })
    }

    fn parse_struct_file(&mut self) -> Result<Vec<StructDecl>, ParseError> {
        let mut structs = Vec::new();
        while !matches!(self.current(), Token::Eof) {
            if !matches!(self.current(), Token::Struct) {
                return Err(self.err("struct file may only contain `struct` declarations"));
            }
            self.bump();
            structs.push(self.parse_struct_decl()?);
        }
        if structs.is_empty() {
            return Err(self.err("struct file must declare at least one struct"));
        }
        Ok(structs)
    }

    fn parse_extern_contract_decl(&mut self) -> Result<ExternContractDecl, ParseError> {
        let attributes = self.parse_attributes_opt()?;
        self.expect("'declare'", |token| matches!(token, Token::Declare))?;
        self.expect("'contract'", |token| matches!(token, Token::Contract))?;
        let name = self.eat_ident()?;
        self.expect("'{'", |token| matches!(token, Token::LBrace))?;
        let mut methods = Vec::new();
        while !matches!(self.current(), Token::RBrace) {
            methods.push(self.parse_extern_method_decl()?);
        }
        self.expect("'}'", |token| matches!(token, Token::RBrace))?;
        Ok(ExternContractDecl {
            attributes,
            name,
            methods,
        })
    }

    fn parse_extern_method_decl(&mut self) -> Result<ExternMethodDecl, ParseError> {
        let attributes = self.parse_attributes_opt()?;
        let return_ty = self.parse_type()?;
        let name = self.eat_ident()?;
        self.expect("'('", |token| matches!(token, Token::LParen))?;
        let params = self.parse_param_list()?;
        self.expect("')'", |token| matches!(token, Token::RParen))?;
        self.expect("';'", |token| matches!(token, Token::Semi))?;
        Ok(ExternMethodDecl {
            attributes,
            return_ty,
            name,
            params,
        })
    }

    fn parse_struct_decl(&mut self) -> Result<StructDecl, ParseError> {
        let name = self.eat_ident()?;
        self.expect("'{'", |token| matches!(token, Token::LBrace))?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut field_spans = Vec::new();
        let mut method_spans = Vec::new();
        while !matches!(self.current(), Token::RBrace) {
            let member_start = self.index;
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
                method_spans.push(self.span_from(member_start));
            } else {
                if !attrs.is_empty() {
                    return Err(self.err("struct fields cannot have attributes"));
                }
                let (init, init_span) = if matches!(self.current(), Token::Eq) {
                    self.bump();
                    let start = self.index;
                    let expr = self.parse_expr()?;
                    (Some(expr), Some(self.span_from(start)))
                } else {
                    (None, None)
                };
                self.expect("';'", |token| matches!(token, Token::Semi))?;
                fields.push(StructField {
                    ty,
                    name: mem_name,
                    init,
                    init_span,
                });
                field_spans.push(self.span_from(member_start));
            }
        }
        self.expect("'}'", |token| matches!(token, Token::RBrace))?;
        Ok(StructDecl {
            name,
            fields,
            methods,
            field_spans,
            method_spans,
        })
    }

    fn eat_attr_name(&mut self) -> Result<String, ParseError> {
        let name = match self.bump() {
            Token::Ident(ident) => ident,
            Token::Hash160 => "hash160".to_string(),
            Token::Hash256 => "hash256".to_string(),
            token => return Err(self.err(format!("expected attribute name, got {token:?}"))),
        };
        Ok(name)
    }

    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        self.expect("'#[", |token| matches!(token, Token::AttrOpen))?;
        let name = self.eat_attr_name()?;
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
            let init_start = self.index;
            let init = self.parse_expr()?;
            let init_span = self.span_from(init_start);
            self.expect("';'", |token| matches!(token, Token::Semi))?;
            return Ok(ContractMember::ConstProp(ConstProp {
                ty,
                name,
                init,
                init_span: Some(init_span),
            }));
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
        let (init, init_span) = if matches!(self.current(), Token::Eq) {
            self.bump();
            let start = self.index;
            let expr = self.parse_expr()?;
            (Some(expr), Some(self.span_from(start)))
        } else {
            (None, None)
        };
        self.expect("';'", |token| matches!(token, Token::Semi))?;
        Ok(ContractMember::Field(ContractField {
            ty,
            name,
            init,
            init_span,
        }))
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

    fn eat_param_name(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Token::Ident(ident) => Ok(ident),
            Token::From => Ok("from".into()),
            token => Err(self.err(format!("expected parameter name, got {token:?}"))),
        }
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut ps = Vec::new();
        if matches!(self.current(), Token::RParen) {
            return Ok(ps);
        }
        loop {
            let ty = self.parse_type()?;
            let name = self.eat_param_name()?;
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
        self.expr_event_stack.push(Vec::new());
        self.expect("'{'", |token| matches!(token, Token::LBrace))?;
        let mut stmts = Vec::new();
        let mut stmt_spans = Vec::new();
        while !matches!(self.current(), Token::RBrace) {
            let stmt_start = self.index;
            stmts.push(self.parse_stmt()?);
            stmt_spans.push(self.span_from(stmt_start));
        }
        self.expect("'}'", |token| matches!(token, Token::RBrace))?;
        Ok(self.finish_block(Block {
            stmts,
            stmt_spans,
            expr_spans: ExprSpanMap::default(),
        }))
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
        let lhs_start = self.index;
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
                InfixOp::Binary(op) => {
                    let expr = Expr::Binary {
                        op,
                        left: Box::new(lhs),
                        right: Box::new(rhs),
                    };
                    self.push_expr_span(self.span_from(lhs_start));
                    expr
                }
                InfixOp::Assign(op) => {
                    let expr = Expr::Assign {
                        target: Box::new(lhs),
                        op,
                        value: Box::new(rhs),
                    };
                    self.push_expr_span(self.span_from(lhs_start));
                    expr
                }
            };
        }
        Ok(lhs)
    }

    fn parse_operand(&mut self) -> Result<Expr, ParseError> {
        let operand_start = self.index;
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
        expr = self.parse_postfix_chain(expr, operand_start)?;
        for unary in unaries.into_iter().rev() {
            expr = Expr::Unary {
                op: unary,
                expr: Box::new(expr),
            };
            self.push_expr_span(self.span_from(operand_start));
        }
        while matches!(self.current(), Token::As) {
            self.bump();
            let ty = self.parse_type()?;
            expr = Expr::Cast {
                expr: Box::new(expr),
                ty,
            };
            self.push_expr_span(self.span_from(operand_start));
        }
        Ok(expr)
    }

    fn parse_postfix_chain(
        &mut self,
        mut expr: Expr,
        expr_start: usize,
    ) -> Result<Expr, ParseError> {
        loop {
            match self.current().clone() {
                Token::Dot => {
                    self.bump();
                    let field = self.eat_ident()?;
                    expr = Expr::Member {
                        base: Box::new(expr),
                        field: field,
                    };
                    self.push_expr_span(self.span_from(expr_start));
                }
                Token::LBracket => {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect("']'", |token| matches!(token, Token::RBracket))?;
                    expr = Expr::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
                    };
                    self.push_expr_span(self.span_from(expr_start));
                }
                Token::LParen => {
                    self.bump();
                    let args = self.parse_expr_list()?;
                    self.expect("')'", |token| matches!(token, Token::RParen))?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                    self.push_expr_span(self.span_from(expr_start));
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
                    self.pop_expr_span();
                    self.push_expr_span(self.span_from(expr_start));
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
        let start = self.index;
        match self.bump() {
            Token::Null => {
                let expr = Expr::Literal(Literal::Null);
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::True => {
                let expr = Expr::Literal(Literal::Bool(true));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::False => {
                let expr = Expr::Literal(Literal::Bool(false));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::Self_ => {
                let expr = Expr::Self_;
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::IntLit(raw) => {
                let expr = Expr::Literal(Literal::Int(raw));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::StringLit(s) => {
                let expr = Expr::Literal(Literal::String(s));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::BufferLit(s) => {
                let expr = Expr::Literal(Literal::Buffer(s));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::Ident(name) => {
                let expr = Expr::Ident(name);
                self.push_expr_span(self.span_from(start));
                Ok(expr)
            }
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect("')'", |token| matches!(token, Token::RParen))?;
                let expr = Expr::Paren(Box::new(inner));
                self.push_expr_span(self.span_from(start));
                Ok(expr)
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
        self.push_expr_span(self.span_from(checkpoint));
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

fn build_expr_span_map(block: &Block, mut events: Vec<Span>) -> ExprSpanMap {
    let mut map = ExprSpanMap::default();
    let mut iter = events.drain(..);
    for stmt in &block.stmts {
        collect_stmt_exprs(stmt, &mut iter, &mut map);
    }
    debug_assert!(iter.next().is_none(), "unmatched expression span events");
    map
}

fn collect_stmt_exprs(stmt: &Stmt, spans: &mut impl Iterator<Item = Span>, map: &mut ExprSpanMap) {
    match stmt {
        Stmt::Var { init, .. } => {
            if let Some(expr) = init {
                collect_expr(expr, spans, map);
            }
        }
        Stmt::Expr(expr) => collect_expr(expr, spans, map),
        Stmt::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr(cond, spans, map);
            collect_block_exprs(then_block, spans, map);
            if let Some(else_block) = else_block {
                collect_block_exprs(else_block, spans, map);
            }
        }
        Stmt::While { cond, body } => {
            collect_expr(cond, spans, map);
            collect_block_exprs(body, spans, map);
        }
        Stmt::ForArray { iter, body, .. } => {
            collect_expr(iter, spans, map);
            collect_block_exprs(body, spans, map);
        }
        Stmt::ForMap {
            map: map_expr,
            body,
            ..
        } => {
            collect_expr(map_expr, spans, map);
            collect_block_exprs(body, spans, map);
        }
        Stmt::Return(Some(expr)) => collect_expr(expr, spans, map),
        Stmt::Return(None) => {}
        Stmt::Emit { args, .. } => {
            for expr in args {
                collect_expr(expr, spans, map);
            }
        }
        Stmt::Block(block) => collect_block_exprs(block, spans, map),
    }
}

fn collect_block_exprs(
    block: &Block,
    spans: &mut impl Iterator<Item = Span>,
    map: &mut ExprSpanMap,
) {
    for stmt in &block.stmts {
        collect_stmt_exprs(stmt, spans, map);
    }
}

fn collect_expr(expr: &Expr, spans: &mut impl Iterator<Item = Span>, map: &mut ExprSpanMap) {
    match expr {
        Expr::Literal(_) | Expr::Ident(_) | Expr::Self_ => {
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Paren(inner) => {
            collect_expr(inner, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Unary { expr: inner, .. } => {
            collect_expr(inner, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Cast { expr: inner, .. } => {
            collect_expr(inner, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Binary { left, right, .. } => {
            collect_expr(left, spans, map);
            collect_expr(right, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Assign { target, value, .. } => {
            collect_expr(target, spans, map);
            collect_expr(value, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Member { base, .. } => {
            collect_expr(base, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Index { base, index } => {
            collect_expr(base, spans, map);
            collect_expr(index, spans, map);
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::Call { callee, args } => {
            collect_expr(callee, spans, map);
            for arg in args {
                collect_expr(arg, spans, map);
            }
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::StructLit { fields, .. } => {
            for (_, expr) in fields {
                collect_expr(expr, spans, map);
            }
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::MapLit { pairs, .. } => {
            for (key, value) in pairs {
                collect_expr(key, spans, map);
                collect_expr(value, spans, map);
            }
            map.insert(expr, spans.next().unwrap_or_default());
        }
        Expr::ArrayLit { elements, .. } => {
            for expr in elements {
                collect_expr(expr, spans, map);
            }
            map.insert(expr, spans.next().unwrap_or_default());
        }
    }
}
