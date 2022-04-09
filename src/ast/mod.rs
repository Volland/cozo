use pest::iterators::{Pair, Pairs};
use pest::Parser as PestParser;
use pest::prec_climber::{Assoc, PrecClimber, Operator};
use crate::parser::Parser;
use crate::parser::Rule;
use lazy_static::lazy_static;
use crate::ast::eval_op::*;
use crate::ast::Expr::{Apply, Const};
use crate::ast::op::Op;
use crate::error::CozoError;
use crate::typing::Typing;
use crate::value::Value;

mod eval_op;
mod op;


lazy_static! {
    static ref PREC_CLIMBER: PrecClimber<Rule> = {
        use Assoc::*;

        PrecClimber::new(vec![
            Operator::new(Rule::op_or, Left),
            Operator::new(Rule::op_and, Left),
            Operator::new(Rule::op_gt, Left) | Operator::new(Rule::op_lt, Left) | Operator::new(Rule::op_ge,Left) | Operator::new(Rule::op_le, Left),
            Operator::new(Rule::op_mod, Left),
            Operator::new(Rule::op_eq, Left) | Operator::new(Rule::op_ne, Left),
            Operator::new(Rule::op_add, Left) | Operator::new(Rule::op_sub, Left),
            Operator::new(Rule::op_mul, Left) | Operator::new(Rule::op_div, Left),
            Operator::new(Rule::op_pow, Assoc::Right),
            Operator::new(Rule::op_coalesce, Assoc::Left)
        ])
    };
}

pub struct Col {
    pub name: String,
    pub typ: Typing,
    pub default: Option<Value<'static>>,
}

pub enum TableDef {
    Node {
        name: String,
        keys: Vec<Col>,
        cols: Vec<Col>,
    },
    Edge {
        src: String,
        dst: String,
        name: String,
        keys: Vec<Col>,
        cols: Vec<Col>,
    },
    Columns {
        attached: String,
        name: String,
        cols: Vec<Col>,
    },
}


#[derive(PartialEq, Debug)]
pub enum Expr<'a> {
    Apply(Op, Vec<Expr<'a>>),
    Const(Value<'a>),
}

impl<'a> Expr<'a> {
    pub fn eval(&self) -> Result<Expr<'a>, CozoError> {
        match self {
            Apply(op, args) => {
                match op {
                    Op::Add => add_exprs(args),
                    Op::Sub => sub_exprs(args),
                    Op::Mul => mul_exprs(args),
                    Op::Div => div_exprs(args),
                    Op::Eq => eq_exprs(args),
                    Op::Neq => ne_exprs(args),
                    Op::Gt => gt_exprs(args),
                    Op::Lt => lt_exprs(args),
                    Op::Ge => ge_exprs(args),
                    Op::Le => le_exprs(args),
                    Op::Neg => negate_expr(args),
                    Op::Minus => minus_expr(args),
                    Op::Mod => mod_exprs(args),
                    Op::Or => or_expr(args),
                    Op::And => and_expr(args),
                    Op::Coalesce => coalesce_exprs(args),
                    Op::Pow => pow_exprs(args),
                    Op::IsNull => is_null_expr(args),
                    Op::NotNull => not_null_expr(args),
                    Op::Call => unimplemented!(),
                }
            }
            Const(v) => Ok(Const(v.clone()))
        }
    }
}

fn build_expr_infix<'a>(lhs: Result<Expr<'a>, CozoError>, op: Pair<Rule>, rhs: Result<Expr<'a>, CozoError>) -> Result<Expr<'a>, CozoError> {
    let lhs = lhs?;
    let rhs = rhs?;
    let op = match op.as_rule() {
        Rule::op_add => Op::Add,
        Rule::op_sub => Op::Sub,
        Rule::op_mul => Op::Mul,
        Rule::op_div => Op::Div,
        Rule::op_eq => Op::Eq,
        Rule::op_ne => Op::Neq,
        Rule::op_or => Op::Or,
        Rule::op_and => Op::And,
        Rule::op_mod => Op::Mod,
        Rule::op_gt => Op::Gt,
        Rule::op_ge => Op::Ge,
        Rule::op_lt => Op::Lt,
        Rule::op_le => Op::Le,
        Rule::op_pow => Op::Pow,
        Rule::op_coalesce => Op::Coalesce,
        _ => unreachable!()
    };
    Ok(Apply(op, vec![lhs, rhs]))
}

#[inline]
fn parse_int(s: &str, radix: u32) -> i64 {
    i64::from_str_radix(&s[2..].replace('_', ""), radix).unwrap()
}

#[inline]
fn parse_raw_string(pairs: Pairs<Rule>) -> Result<String, CozoError> {
    Ok(pairs.into_iter().next().unwrap().as_str().to_string())
}

#[inline]
fn parse_quoted_string(pairs: Pairs<Rule>) -> Result<String, CozoError> {
    let mut ret = String::with_capacity(pairs.as_str().len());
    for pair in pairs {
        let s = pair.as_str();
        match s {
            r#"\""# => ret.push('"'),
            r"\\" => ret.push('\\'),
            r"\/" => ret.push('/'),
            r"\b" => ret.push('\x08'),
            r"\f" => ret.push('\x0c'),
            r"\n" => ret.push('\n'),
            r"\r" => ret.push('\r'),
            r"\t" => ret.push('\t'),
            s if s.starts_with(r"\u") => {
                let code = parse_int(s, 16) as u32;
                let ch = char::from_u32(code).ok_or(CozoError::InvalidUtfCode)?;
                ret.push(ch);
            }
            s if s.starts_with('\\') => return Err(CozoError::InvalidEscapeSequence),
            s => ret.push_str(s)
        }
    }
    Ok(ret)
}


#[inline]
fn parse_s_quoted_string(pairs: Pairs<Rule>) -> Result<String, CozoError> {
    let mut ret = String::with_capacity(pairs.as_str().len());
    for pair in pairs {
        let s = pair.as_str();
        match s {
            r#"\'"# => ret.push('\''),
            r"\\" => ret.push('\\'),
            r"\/" => ret.push('/'),
            r"\b" => ret.push('\x08'),
            r"\f" => ret.push('\x0c'),
            r"\n" => ret.push('\n'),
            r"\r" => ret.push('\r'),
            r"\t" => ret.push('\t'),
            s if s.starts_with(r"\u") => {
                let code = parse_int(s, 16) as u32;
                let ch = char::from_u32(code).ok_or(CozoError::InvalidUtfCode)?;
                ret.push(ch);
            }
            s if s.starts_with('\\') => return Err(CozoError::InvalidEscapeSequence),
            s => ret.push_str(s)
        }
    }
    Ok(ret)
}

fn build_expr_primary(pair: Pair<Rule>) -> Result<Expr, CozoError> {
    match pair.as_rule() {
        Rule::expr => build_expr_primary(pair.into_inner().next().unwrap()),
        Rule::term => build_expr_primary(pair.into_inner().next().unwrap()),
        Rule::grouping => build_expr(pair.into_inner().next().unwrap()),

        Rule::unary => {
            let mut inner = pair.into_inner();
            let op = inner.next().unwrap().as_rule();
            let term = build_expr_primary(inner.next().unwrap())?;
            Ok(Apply(match op {
                Rule::negate => Op::Neg,
                Rule::minus => Op::Minus,
                _ => unreachable!()
            }, vec![term]))
        }

        Rule::pos_int => Ok(Const(Value::Int(pair.as_str().replace('_', "").parse::<i64>()?))),
        Rule::hex_pos_int => Ok(Const(Value::Int(parse_int(pair.as_str(), 16)))),
        Rule::octo_pos_int => Ok(Const(Value::Int(parse_int(pair.as_str(), 8)))),
        Rule::bin_pos_int => Ok(Const(Value::Int(parse_int(pair.as_str(), 2)))),
        Rule::dot_float | Rule::sci_float => Ok(Const(Value::Float(pair.as_str().replace('_', "").parse::<f64>()?))),
        Rule::null => Ok(Const(Value::Null)),
        Rule::boolean => Ok(Const(Value::Bool(pair.as_str() == "true"))),
        Rule::quoted_string => Ok(Const(Value::OwnString(Box::new(parse_quoted_string(pair.into_inner().next().unwrap().into_inner())?)))),
        Rule::s_quoted_string => Ok(Const(Value::OwnString(Box::new(parse_s_quoted_string(pair.into_inner().next().unwrap().into_inner())?)))),
        Rule::raw_string => Ok(Const(Value::OwnString(Box::new(parse_raw_string(pair.into_inner())?)))),
        _ => {
            println!("{:#?}", pair);
            unimplemented!()
        }
    }
}

fn build_expr(pair: Pair<Rule>) -> Result<Expr, CozoError> {
    PREC_CLIMBER.climb(pair.into_inner(), build_expr_primary, build_expr_infix)
}

pub fn parse_expr_from_str(inp: &str) -> Result<Expr, CozoError> {
    let expr_tree = Parser::parse(Rule::expr, inp)?.next().unwrap();
    build_expr(expr_tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_string() {
        println!("{:#?}", parse_expr_from_str(r#####"r#"x"#"#####))
    }

    #[test]
    fn parse_literals() {
        assert_eq!(parse_expr_from_str("1").unwrap(), Const(Value::Int(1)));
        assert_eq!(parse_expr_from_str("12_3").unwrap(), Const(Value::Int(123)));
        assert_eq!(parse_expr_from_str("0xaf").unwrap(), Const(Value::Int(0xaf)));
        assert_eq!(parse_expr_from_str("0xafcE_f").unwrap(), Const(Value::Int(0xafcef)));
        assert_eq!(parse_expr_from_str("0o1234_567").unwrap(), Const(Value::Int(0o1234567)));
        assert_eq!(parse_expr_from_str("0o0001234_567").unwrap(), Const(Value::Int(0o1234567)));
        assert_eq!(parse_expr_from_str("0b101010").unwrap(), Const(Value::Int(0b101010)));

        assert_eq!(parse_expr_from_str("0.0").unwrap(), Const(Value::Float(0.)));
        assert_eq!(parse_expr_from_str("10.022_3").unwrap(), Const(Value::Float(10.0223)));
        assert_eq!(parse_expr_from_str("10.022_3e-100").unwrap(), Const(Value::Float(10.0223e-100)));

        assert_eq!(parse_expr_from_str("null").unwrap(), Const(Value::Null));
        assert_eq!(parse_expr_from_str("true").unwrap(), Const(Value::Bool(true)));
        assert_eq!(parse_expr_from_str("false").unwrap(), Const(Value::Bool(false)));
        assert_eq!(parse_expr_from_str(r#""x \n \ty \"""#).unwrap(), Const(Value::RefString("x \n \ty \"")));
        assert_eq!(parse_expr_from_str(r#""x'""#).unwrap(), Const(Value::RefString("x'")));
        assert_eq!(parse_expr_from_str(r#"'"x"'"#).unwrap(), Const(Value::RefString(r##""x""##)));
        assert_eq!(parse_expr_from_str(r#####"r###"x"yz"###"#####).unwrap(), Const(Value::RefString(r##"x"yz"##)));
    }

    #[test]
    fn operators() {
        println!("{:#?}", parse_expr_from_str("1/10+(-2+3)*4^5").unwrap().eval().unwrap());
        println!("{:#?}", parse_expr_from_str("true && false").unwrap().eval().unwrap());
        println!("{:#?}", parse_expr_from_str("true || false").unwrap().eval().unwrap());
        println!("{:#?}", parse_expr_from_str("true || null").unwrap().eval().unwrap());
        println!("{:#?}", parse_expr_from_str("null || true").unwrap().eval().unwrap());
        println!("{:#?}", parse_expr_from_str("true && null").unwrap().eval().unwrap());
    }
}