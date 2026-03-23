use std::fmt;

#[derive(Debug, Clone)]
pub enum Type {
    I32,
    I64,
    F32,
    F64,
    Bool,
    Str,
    Void,
    Array { element: Box<Type>, size: usize },
    Map { key: Box<Type>, value: Box<Type> },
    Struct(String),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::Void => write!(f, "void"),
            Type::Array { element, size } => write!(f, "[{}; {}]", element, size),
            Type::Map { key, value } => write!(f, "map[{}, {}]", key, value),
            Type::Struct(name) => write!(f, "{}", name),
        }
    }
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::Neg => write!(f, "-"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::Greater => ">",
            BinaryOp::LessEqual => "<=",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And    => "&&",
            BinaryOp::Or     => "||",
            BinaryOp::Mod    => "%",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr  => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl    => "<<",
            BinaryOp::Shr    => ">>",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub enum Expression {
    IntLiteral(i64),
    FloatLiteral(f64),
    BoolLiteral(bool),
    StringLiteral(String),
    ArrayLiteral(Vec<Expression>),
    Identifier(String),
    Index {
        name: String,
        index: Box<Expression>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },
    Call {
        name: String,
        arguments: Vec<Expression>,
    },
    Cast {
        value: Box<Expression>,
        typ: Type,
    },
    StructLiteral {
        name: String,
        fields: Vec<(String, Expression)>,
    },
    FieldAccess {
        object: Box<Expression>,
        field: String,
    },
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expression::IntLiteral(n) => write!(f, "{}", n),  // i64
            Expression::FloatLiteral(v) => write!(f, "{}", v),
            Expression::BoolLiteral(b) => write!(f, "{}", b),
            Expression::StringLiteral(s) => write!(f, "\"{}\"", s),
            Expression::ArrayLiteral(elems) => {
                write!(f, "[")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", e)?;
                }
                write!(f, "]")
            }
            Expression::Identifier(name) => write!(f, "{}", name),
            Expression::Index { name, index } => write!(f, "{}[{}]", name, index),
            Expression::Unary { op, operand } => write!(f, "({} {})", op, operand),
            Expression::Binary { left, op, right } => write!(f, "({} {} {})", left, op, right),
            Expression::Call { name, arguments } => {
                write!(f, "{}(", name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::Cast { value, typ } => write!(f, "({} as {})", value, typ),
            Expression::StructLiteral { name, fields } => {
                write!(f, "{} {{", name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Expression::FieldAccess { object, field } => write!(f, "{}.{}", object, field),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    VariableDecl {
        name: String,
        typ: Type,
        initializer: Option<Expression>,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Return {
        value: Option<Expression>,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    Print {
        values: Vec<Expression>,
    },
    Input {
        name: String,
        typ: Type,
    },
    InputIndex {
        name: String,
        index: Expression,
    },
    For {
        var: String,
        start: Expression,
        end: Expression,
        body: Vec<Statement>,
    },
    IndexAssign {
        name: String,
        index: Expression,
        value: Expression,
    },
    Break,
    Continue,
    Expr(Expression), // standalone expression (e.g. function call used as statement)
    FieldAssign {
        /// Full dot-separated path: ["a", "b", "c"] represents `a.b.c = val`.
        /// Must have at least two elements (object + field).
        path: Vec<String>,
        value: Expression,
    },
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Statement::VariableDecl { name, typ, initializer } => {
                write!(f, "let {}: {}", name, typ)?;
                if let Some(init) = initializer {
                    write!(f, " = {}", init)?;
                }
                write!(f, ";")
            }
            Statement::Assign { name, value } => write!(f, "{} = {};", name, value),
            Statement::Return { value } => {
                write!(f, "return")?;
                if let Some(v) = value {
                    write!(f, " {}", v)?;
                }
                write!(f, ";")
            }
            Statement::If { condition, then_body, else_body } => {
                writeln!(f, "if {} {{", condition)?;
                for stmt in then_body {
                    writeln!(f, "  {}", stmt)?;
                }
                write!(f, "}}")?;
                if let Some(else_b) = else_body {
                    writeln!(f, " else {{")?;
                    for stmt in else_b {
                        writeln!(f, "  {}", stmt)?;
                    }
                    write!(f, "}}")?;
                }
                Ok(())
            }
            Statement::While { condition, body } => {
                writeln!(f, "while {} {{", condition)?;
                for stmt in body {
                    writeln!(f, "  {}", stmt)?;
                }
                write!(f, "}}")
            }
            Statement::Print { values } => {
                write!(f, "print ")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, ";")
            }
            Statement::Input { name, typ } => write!(f, "input {}: {};", name, typ),
            Statement::InputIndex { name, index } => write!(f, "input {}[{}];", name, index),
            Statement::For { var, start, end, body } => {
                writeln!(f, "for {} in {}..{} {{", var, start, end)?;
                for stmt in body { writeln!(f, "  {}", stmt)?; }
                write!(f, "}}")
            }
            Statement::IndexAssign { name, index, value } => write!(f, "{}[{}] = {};", name, index, value),
            Statement::Break       => write!(f, "break;"),
            Statement::Continue    => write!(f, "continue;"),
            Statement::Expr(expr)  => write!(f, "{};", expr),
            Statement::FieldAssign { path, value } => write!(f, "{} = {};", path.join("."), value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub typ: Type,
}

impl fmt::Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.typ)
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "fn {}(", self.name)?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param)?;
        }
        writeln!(f, ") -> {} {{", self.return_type)?;
        for stmt in &self.body {
            writeln!(f, "  {}", stmt)?;
        }
        write!(f, "}}")
    }
}

#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub name: String,
    pub typ: Type,
    pub initializer: Option<Expression>,
}

impl fmt::Display for GlobalVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "let {}: {}", self.name, self.typ)?;
        if let Some(init) = &self.initializer {
            write!(f, " = {}", init)?;
        }
        write!(f, ";")
    }
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub typ: Type,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
}

impl fmt::Display for StructDef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "struct {} {{", self.name)?;
        for field in &self.fields {
            writeln!(f, "  {}: {},", field.name, field.typ)?;
        }
        write!(f, "}}")
    }
}

#[derive(Debug, Clone)]
pub struct ExternFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
}

impl fmt::Display for ExternFunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "extern fn {}(", self.name)?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}", param)?;
        }
        write!(f, ") -> {};", self.return_type)
    }
}

#[derive(Debug)]
pub struct Program {
    pub globals: Vec<GlobalVar>,
    pub functions: Vec<Function>,
    pub externs: Vec<ExternFunction>,
    pub structs: Vec<StructDef>,
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for s in &self.structs {
            writeln!(f, "{}", s)?;
            writeln!(f)?;
        }
        for ext in &self.externs {
            writeln!(f, "{}", ext)?;
        }
        if !self.externs.is_empty() { writeln!(f)?; }
        for g in &self.globals {
            writeln!(f, "{}", g)?;
        }
        if !self.globals.is_empty() { writeln!(f)?; }
        for func in &self.functions {
            writeln!(f, "{}", func)?;
            writeln!(f)?;
        }
        Ok(())
    }
}
