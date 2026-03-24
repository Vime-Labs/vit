use crate::ast::*;
use crate::lexer::{Token, TokenType};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, expected: TokenType) -> Result<(), String> {
        if std::mem::discriminant(&self.current().typ) != std::mem::discriminant(&expected) {
            return Err(format!(
                "Expected {:?}, got {:?} at {}:{}",
                expected,
                self.current().typ,
                self.current().line,
                self.current().column
            ));
        }
        self.advance();
        Ok(())
    }

    fn parse_program(&mut self) -> Result<Program, String> {
        let mut globals = Vec::new();
        let mut functions = Vec::new();
        let mut externs = Vec::new();
        let mut structs = Vec::new();

        while !matches!(self.current().typ, TokenType::Eof) {
            if matches!(self.current().typ, TokenType::Fn) {
                functions.push(self.parse_function()?);
            } else if matches!(self.current().typ, TokenType::Let) {
                globals.push(self.parse_global_decl()?);
            } else if matches!(self.current().typ, TokenType::Extern) {
                externs.push(self.parse_extern_function()?);
            } else if matches!(self.current().typ, TokenType::Struct) {
                structs.push(self.parse_struct_def()?);
            } else {
                return Err(format!(
                    "Expected 'fn', 'let', 'extern' or 'struct' at top level, got {:?} at {}:{}",
                    self.current().typ, self.current().line, self.current().column
                ));
            }
        }

        Ok(Program { globals, functions, externs, structs })
    }

    fn parse_struct_def(&mut self) -> Result<StructDef, String> {
        self.expect(TokenType::Struct)?;
        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected struct name after 'struct'".to_string()),
        };
        self.advance();
        self.expect(TokenType::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.current().typ, TokenType::RBrace | TokenType::Eof) {
            let fname = match &self.current().typ {
                TokenType::Identifier(s) => s.clone(),
                _ => return Err("Expected field name in struct definition".to_string()),
            };
            self.advance();
            self.expect(TokenType::Colon)?;
            let ftype = self.parse_type()?;
            fields.push(StructField { name: fname, typ: ftype });
            if matches!(self.current().typ, TokenType::Comma) {
                self.advance();
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(StructDef { name, fields })
    }

    fn parse_extern_function(&mut self) -> Result<ExternFunction, String> {
        self.expect(TokenType::Extern)?;
        self.expect(TokenType::Fn)?;

        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected function name after 'extern fn'".to_string()),
        };
        self.advance();

        self.expect(TokenType::LParen)?;
        let parameters = self.parse_parameters()?;
        self.expect(TokenType::RParen)?;

        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type()?;
        self.expect(TokenType::Semicolon)?;

        Ok(ExternFunction { name, parameters, return_type })
    }

    fn parse_global_decl(&mut self) -> Result<GlobalVar, String> {
        self.expect(TokenType::Let)?;
        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name in global declaration".to_string()),
        };
        self.advance();
        self.expect(TokenType::Colon)?;
        let typ = self.parse_type()?;
        let initializer = if matches!(self.current().typ, TokenType::Assign) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.expect(TokenType::Semicolon)?;
        Ok(GlobalVar { name, typ, initializer })
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        self.expect(TokenType::Fn)?;

        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected function name".to_string()),
        };
        self.advance();

        self.expect(TokenType::LParen)?;
        let parameters = self.parse_parameters()?;
        self.expect(TokenType::RParen)?;

        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type()?;

        self.expect(TokenType::LBrace)?;
        let body = self.parse_block()?;
        self.expect(TokenType::RBrace)?;

        Ok(Function {
            name,
            parameters,
            return_type,
            body,
        })
    }

    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, String> {
        let mut parameters = Vec::new();

        while !matches!(self.current().typ, TokenType::RParen) {
            let name = match &self.current().typ {
                TokenType::Identifier(s) => s.clone(),
                _ => return Err("Expected parameter name".to_string()),
            };
            self.advance();

            self.expect(TokenType::Colon)?;
            let typ = self.parse_type()?;

            parameters.push(Parameter { name, typ });

            if matches!(self.current().typ, TokenType::RParen) {
                break;
            }

            self.expect(TokenType::Comma)?;
        }

        Ok(parameters)
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        match &self.current().typ {
            TokenType::I32  => { self.advance(); Ok(Type::I32) }
            TokenType::I64  => { self.advance(); Ok(Type::I64) }
            TokenType::F32  => { self.advance(); Ok(Type::F32) }
            TokenType::F64  => { self.advance(); Ok(Type::F64) }
            TokenType::Bool => { self.advance(); Ok(Type::Bool) }
            TokenType::Str  => { self.advance(); Ok(Type::Str) }
            TokenType::Void => { self.advance(); Ok(Type::Void) }
            TokenType::LBracket => {
                self.advance(); // consume '['
                let element = Box::new(self.parse_type()?);
                self.expect(TokenType::Semicolon)?;
                let size = match &self.current().typ {
                    TokenType::IntLiteral(n) => *n as usize,  // i64 → usize
                    _ => return Err("Expected array size (integer literal)".to_string()),
                };
                self.advance();
                self.expect(TokenType::RBracket)?;
                Ok(Type::Array { element, size })
            }
            TokenType::Map => {
                self.advance(); // consume 'map'
                self.expect(TokenType::LBracket)?;
                let key = Box::new(self.parse_type()?);
                self.expect(TokenType::Comma)?;
                let value = Box::new(self.parse_type()?);
                self.expect(TokenType::RBracket)?;
                Ok(Type::Map { key, value })
            }
            TokenType::Identifier(s) => {
                let name = s.clone();
                self.advance();
                Ok(Type::Struct(name))
            }
            _ => Err(format!("Expected type, got {:?} at {}:{}", self.current().typ, self.current().line, self.current().column)),
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Statement>, String> {
        let mut statements = Vec::new();

        while !matches!(self.current().typ, TokenType::RBrace | TokenType::Eof) {
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        match &self.current().typ {
            TokenType::Let      => self.parse_variable_decl(),
            TokenType::Return   => self.parse_return_stmt(),
            TokenType::If       => self.parse_if_stmt(),
            TokenType::While    => self.parse_while_stmt(),
            TokenType::For      => self.parse_for_stmt(),
            TokenType::Print    => self.parse_print_stmt(),
            TokenType::Input    => self.parse_input_stmt(),
            TokenType::Break    => { self.advance(); self.expect(TokenType::Semicolon)?; Ok(Statement::Break) }
            TokenType::Continue => { self.advance(); self.expect(TokenType::Semicolon)?; Ok(Statement::Continue) }
            TokenType::Identifier(_) => {
                match self.peek(1).map(|t| &t.typ) {
                    Some(TokenType::Assign)         => self.parse_assign_stmt(),
                    Some(TokenType::LBracket)       => self.parse_index_assign_stmt(),
                    Some(TokenType::PlusAssign)     => self.parse_compound_assign(BinaryOp::Add),
                    Some(TokenType::MinusAssign)    => self.parse_compound_assign(BinaryOp::Sub),
                    Some(TokenType::StarAssign)     => self.parse_compound_assign(BinaryOp::Mul),
                    Some(TokenType::SlashAssign)    => self.parse_compound_assign(BinaryOp::Div),
                    Some(TokenType::PercentAssign)  => self.parse_compound_assign(BinaryOp::Mod),
                    Some(TokenType::LParen)         => self.parse_expr_stmt(), // function call as statement
                    Some(TokenType::Dot)            => self.parse_field_assign_stmt(),
                    _ => Err(format!("Unexpected token {:?}", self.current().typ)),
                }
            }
            _ => Err(format!("Unexpected token {:?}", self.current().typ)),
        }
    }

    fn parse_expr_stmt(&mut self) -> Result<Statement, String> {
        let expr = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::Expr(expr))
    }

    fn parse_compound_assign(&mut self, op: BinaryOp) -> Result<Statement, String> {
        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name".to_string()),
        };
        self.advance(); // consume name
        self.advance(); // consume op=
        let rhs = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        // Desugar: x op= rhs  →  x = x op rhs
        Ok(Statement::Assign {
            name: name.clone(),
            value: Expression::Binary {
                left: Box::new(Expression::Identifier(name)),
                op,
                right: Box::new(rhs),
            },
        })
    }

    fn parse_field_assign_stmt(&mut self) -> Result<Statement, String> {
        // Consume `ident.field1.field2...fieldN = value;`
        let mut path = vec![match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected object name".to_string()),
        }];
        self.advance();
        // Consume one or more `.field` segments
        loop {
            self.expect(TokenType::Dot)?;
            let seg = match &self.current().typ {
                TokenType::Identifier(s) => s.clone(),
                _ => return Err("Expected field name after '.'".to_string()),
            };
            self.advance();
            path.push(seg);
            // If the next token is another '.', continue the chain; otherwise expect '='
            if !matches!(self.current().typ, TokenType::Dot) {
                break;
            }
        }
        self.expect(TokenType::Assign)?;
        let value = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::FieldAssign { path, value })
    }

    fn parse_index_assign_stmt(&mut self) -> Result<Statement, String> {
        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected array name".to_string()),
        };
        self.advance();
        self.expect(TokenType::LBracket)?;
        let index = self.parse_expression()?;
        self.expect(TokenType::RBracket)?;
        self.expect(TokenType::Assign)?;
        let value = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::IndexAssign { name, index, value })
    }

    fn parse_assign_stmt(&mut self) -> Result<Statement, String> {
        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name".to_string()),
        };
        self.advance();
        self.expect(TokenType::Assign)?;
        let value = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::Assign { name, value })
    }

    fn parse_variable_decl(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::Let)?;

        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name".to_string()),
        };
        self.advance();

        self.expect(TokenType::Colon)?;
        let typ = self.parse_type()?;

        let initializer = if matches!(self.current().typ, TokenType::Assign) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.expect(TokenType::Semicolon)?;

        Ok(Statement::VariableDecl {
            name,
            typ,
            initializer,
        })
    }

    fn parse_return_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::Return)?;

        let value = if !matches!(self.current().typ, TokenType::Semicolon) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.expect(TokenType::Semicolon)?;

        Ok(Statement::Return { value })
    }

    fn parse_if_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::If)?;

        let condition = self.parse_expression()?;

        self.expect(TokenType::LBrace)?;
        let then_body = self.parse_block()?;
        self.expect(TokenType::RBrace)?;

        let else_body = if matches!(self.current().typ, TokenType::Else) {
            self.advance();
            if matches!(self.current().typ, TokenType::If) {
                // else if — parse the inner if as a single statement in the else body
                Some(vec![self.parse_if_stmt()?])
            } else {
                self.expect(TokenType::LBrace)?;
                let body = self.parse_block()?;
                self.expect(TokenType::RBrace)?;
                Some(body)
            }
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_for_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::For)?;

        let var = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name after 'for'".to_string()),
        };
        self.advance();

        self.expect(TokenType::In)?;
        let start = self.parse_expression()?;
        self.expect(TokenType::DotDot)?;
        let end = self.parse_expression()?;

        self.expect(TokenType::LBrace)?;
        let body = self.parse_block()?;
        self.expect(TokenType::RBrace)?;

        Ok(Statement::For { var, start, end, body })
    }

    fn parse_while_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::While)?;

        let condition = self.parse_expression()?;

        self.expect(TokenType::LBrace)?;
        let body = self.parse_block()?;
        self.expect(TokenType::RBrace)?;

        Ok(Statement::While { condition, body })
    }

    fn parse_input_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::Input)?;

        let name = match &self.current().typ {
            TokenType::Identifier(s) => s.clone(),
            _ => return Err("Expected variable name after 'input'".to_string()),
        };
        self.advance();

        // input arr[i];  — write into array element
        if matches!(self.current().typ, TokenType::LBracket) {
            self.advance();
            let index = self.parse_expression()?;
            self.expect(TokenType::RBracket)?;
            self.expect(TokenType::Semicolon)?;
            return Ok(Statement::InputIndex { name, index });
        }

        // input x: type;  — declare new variable
        self.expect(TokenType::Colon)?;
        let typ = self.parse_type()?;
        self.expect(TokenType::Semicolon)?;

        Ok(Statement::Input { name, typ })
    }

    fn parse_print_stmt(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::Print)?;
        let mut values = vec![self.parse_expression()?];
        while matches!(self.current().typ, TokenType::Comma) {
            self.advance();
            values.push(self.parse_expression()?);
        }
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::Print { values })
    }

    fn parse_expression(&mut self) -> Result<Expression, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_and()?;

        while matches!(self.current().typ, TokenType::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expression::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_comparison()?;

        while matches!(self.current().typ, TokenType::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expression::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_additive()?;

        while let Some(op) = match &self.current().typ {
            TokenType::Equal => Some(BinaryOp::Equal),
            TokenType::NotEqual => Some(BinaryOp::NotEqual),
            TokenType::Less => Some(BinaryOp::Less),
            TokenType::Greater => Some(BinaryOp::Greater),
            TokenType::LessEqual => Some(BinaryOp::LessEqual),
            TokenType::GreaterEqual => Some(BinaryOp::GreaterEqual),
            _ => None,
        } {
            self.advance();
            let right = self.parse_additive()?;
            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_multiplicative()?;

        while let Some(op) = match &self.current().typ {
            TokenType::Plus => Some(BinaryOp::Add),
            TokenType::Minus => Some(BinaryOp::Sub),
            _ => None,
        } {
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_unary()?;

        while let Some(op) = match &self.current().typ {
            TokenType::Star      => Some(BinaryOp::Mul),
            TokenType::Slash     => Some(BinaryOp::Div),
            TokenType::Percent   => Some(BinaryOp::Mod),
            TokenType::Ampersand => Some(BinaryOp::BitAnd),
            TokenType::Pipe      => Some(BinaryOp::BitOr),
            TokenType::Caret     => Some(BinaryOp::BitXor),
            TokenType::LShift    => Some(BinaryOp::Shl),
            TokenType::RShift    => Some(BinaryOp::Shr),
            _ => None,
        } {
            self.advance();
            let right = self.parse_primary()?;
            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expression, String> {
        if matches!(self.current().typ, TokenType::Bang) {
            self.advance();
            let operand = self.parse_unary()?;
            Ok(Expression::Unary { op: UnaryOp::Not, operand: Box::new(operand) })
        } else if matches!(self.current().typ, TokenType::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            Ok(Expression::Unary { op: UnaryOp::Neg, operand: Box::new(operand) })
        } else {
            self.parse_cast()
        }
    }

    fn parse_cast(&mut self) -> Result<Expression, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.current().typ, TokenType::As) {
                self.advance();
                let typ = self.parse_type()?;
                expr = Expression::Cast { value: Box::new(expr), typ };
            } else if matches!(self.current().typ, TokenType::Dot) {
                self.advance();
                let field = match &self.current().typ {
                    TokenType::Identifier(s) => s.clone(),
                    _ => return Err("Expected field name after '.'".to_string()),
                };
                self.advance();
                expr = Expression::FieldAccess { object: Box::new(expr), field };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression, String> {
        match &self.current().typ {
            TokenType::IntLiteral(n) => {
                let val = *n;
                self.advance();
                Ok(Expression::IntLiteral(val))
            }
            TokenType::FloatLiteral(v) => {
                let val = *v;
                self.advance();
                Ok(Expression::FloatLiteral(val))
            }
            TokenType::LBracket => {
                self.advance(); // consume '['
                let mut elements = Vec::new();
                while !matches!(self.current().typ, TokenType::RBracket) {
                    elements.push(self.parse_expression()?);
                    if matches!(self.current().typ, TokenType::Comma) {
                        self.advance();
                    } else if !matches!(self.current().typ, TokenType::RBracket) {
                        return Err("Expected ',' or ']' in array literal".to_string());
                    }
                }
                self.expect(TokenType::RBracket)?;
                Ok(Expression::ArrayLiteral(elements))
            }
            TokenType::True => {
                self.advance();
                Ok(Expression::BoolLiteral(true))
            }
            TokenType::False => {
                self.advance();
                Ok(Expression::BoolLiteral(false))
            }
            TokenType::StringLiteral(s) => {
                let val = s.clone();
                self.advance();
                Ok(Expression::StringLiteral(val))
            }
            TokenType::Identifier(s) => {
                let name = s.clone();
                self.advance();

                // Check for function call, array index, or struct literal
                if matches!(self.current().typ, TokenType::LBracket) {
                    self.advance();
                    let index = self.parse_expression()?;
                    self.expect(TokenType::RBracket)?;
                    Ok(Expression::Index { name, index: Box::new(index) })
                } else if matches!(self.current().typ, TokenType::LParen) {
                    self.advance();
                    let mut arguments = Vec::new();

                    while !matches!(self.current().typ, TokenType::RParen) {
                        arguments.push(self.parse_expression()?);

                        if matches!(self.current().typ, TokenType::Comma) {
                            self.advance();
                        } else if !matches!(self.current().typ, TokenType::RParen) {
                            return Err("Expected ',' or ')' in function call".to_string());
                        }
                    }

                    self.expect(TokenType::RParen)?;
                    Ok(Expression::Call { name, arguments })
                } else if matches!(self.current().typ, TokenType::LBrace)
                    && name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    && matches!(
                        self.peek(1).map(|t| &t.typ),
                        Some(TokenType::RBrace)            // Foo {}
                            | Some(TokenType::Identifier(_)) // Foo { field: ...
                    )
                    && (matches!(
                        self.peek(1).map(|t| &t.typ),
                        Some(TokenType::RBrace)
                    ) || matches!(
                        self.peek(2).map(|t| &t.typ),
                        Some(TokenType::Colon)
                    ))
                {
                    // Struct literal: Name { field: val, ... }
                    self.advance(); // consume '{'
                    let mut fields = Vec::new();
                    while !matches!(self.current().typ, TokenType::RBrace | TokenType::Eof) {
                        let fname = match &self.current().typ {
                            TokenType::Identifier(s) => s.clone(),
                            _ => return Err("Expected field name in struct literal".to_string()),
                        };
                        self.advance();
                        self.expect(TokenType::Colon)?;
                        let fval = self.parse_expression()?;
                        fields.push((fname, fval));
                        if matches!(self.current().typ, TokenType::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(TokenType::RBrace)?;
                    Ok(Expression::StructLiteral { name, fields })
                } else {
                    Ok(Expression::Identifier(name))
                }
            }
            TokenType::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(TokenType::RParen)?;
                Ok(expr)
            }
            _ => Err(format!("Unexpected token {:?}", self.current().typ)),
        }
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, String> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}
