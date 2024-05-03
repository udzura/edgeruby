extern crate nom;

#[derive(Debug)]
pub struct FuncDef {
    pub name: String,
    pub argstype: Vec<String>,
    pub rettype: String,
}

impl FuncDef {
    pub fn args_decl(&self) -> &str {
        if self.argstype.len() == 0 {
            return "";
        }

        let converted: Vec<String> = self
            .argstype
            .iter()
            .enumerate()
            .map(|(idx, arg)| match arg.as_str() {
                "Integer" => format!("a{}: i32", idx),
                "Float" => format!("a{}: f32", idx),
                "String" => format!("p{0}: *const u8, l{0}: usize", idx),
                _ => {
                    unimplemented!("unsupported arg type")
                }
            })
            .collect();
        converted.join(", ").leak()
    }

    pub fn args_let_vec(&self) -> &str {
        if self.argstype.len() == 0 {
            return "vec![]";
        }

        let converted: Vec<String> = self
            .argstype
            .iter()
            .enumerate()
            .map(|(idx, arg)| match arg.as_str() {
                "Integer" => format!("std::rc::Rc::new(RObject::RInteger(a{} as i64))", idx),
                "Float" => format!("std::rc::Rc::new(RObject::RFloat(a{} as f64))", idx),
                "String" => format!("std::rc::Rc::new(RObject::RString(a{}.to_owned()))", idx),
                _ => {
                    unimplemented!("unsupported arg type")
                }
            })
            .collect();
        format!("vec![{}]", converted.join(", ")).leak()
    }

    pub fn str_args_converter(&self) -> &str {
        if self.argstype.len() == 0 {
            return "";
        }
        let mut buf = String::new();

        for (idx, arg) in self.argstype.iter().enumerate() {
            match arg.as_str() {
                "String" => {
                    buf.push_str(&format!(
                        "
let a{0} = unsafe {{
    let s = std::slice::from_raw_parts(p{0}, l{0} as usize);
    std::str::from_utf8(s).expect(\"invalid utf8\")
}};
",
                        idx
                    ));
                }
                _ => {
                    // skip
                }
            }
        }

        buf.leak()
    }

    pub fn rettype_decl(&self) -> &str {
        match self.rettype.as_str() {
            "void" => "-> ()",
            "Integer" => "-> i32",
            "Float" => "-> f32",
            "String" => "-> *const u8",
            _ => {
                unimplemented!("unsupported arg type")
            }
        }
    }

    pub fn handle_retval(&self) -> &str {
        match self.rettype.as_str() {
            "String" => {
                let mut buf = String::new();
                buf.push_str("let mut retval: String = retval.as_ref().try_into().unwrap();\n");
                // TODO: handle string length
                buf.push_str("retval.push('\0');\n");
                buf.push_str("retval.as_str().as_ptr()\n");
                buf.leak()
            }
            _ => "retval.as_ref().try_into().unwrap()",
        }
    }

    // for function importer
    pub fn imoprted_body(&self) -> &str {
        let mut buf = String::new();
        for (i, typ) in self.argstype.iter().enumerate() {
            let tmp = match typ.as_str() {
                "String" => {
                    let mut buf = String::new();
                    buf.push_str(&format!(
                        "let a{0}: String = args[{0}].clone().as_ref().try_into().unwrap();\n",
                        i
                    ));
                    buf.push_str(&format!("let p{0} = a{0}.as_str().as_ptr();\n", i));
                    buf.push_str(&format!("let l{0} = a{0}.as_str().len();\n", i));
                    buf
                }
                _ => format!(
                    "let a{0} = args[{0}].clone().as_ref().try_into().unwrap();\n",
                    i,
                ),
            };
            buf.push_str(&tmp);
        }
        let call_arg = self
            .argstype
            .iter()
            .enumerate()
            .map(|(i, typ)| match typ.as_str() {
                "String" => format!("p{0}, l{0}", i),
                _ => format!("a{}", i),
            })
            .collect::<Vec<String>>()
            .join(",");
        buf.push_str(&format!(
            "let r0 = unsafe {{ {}({}) }};\n",
            &self.name, call_arg
        ));
        let ret_mruby_type = match self.rettype.as_str() {
            "Integer" => "RObject::RInteger(r0 as i64)",
            "void" => "RObject::Nil",
            _ => unimplemented!("unsupported arg type"),
        };
        buf.push_str(&format!("Rc::new({})\n", ret_mruby_type));
        buf.leak()
    }
}

use nom::branch::alt;
use nom::branch::permutation;
use nom::bytes::complete::tag;
use nom::character::complete::*;
// use nom::combinator::opt;
use nom::error::context;
use nom::error::VerboseError;
use nom::multi::*;
use nom::sequence::tuple;
use nom::IResult;

type Res<T, U> = IResult<T, U, VerboseError<T>>;

fn def(input: &str) -> Res<&str, ()> {
    context("def", tag("def"))(input).map(|(s, _)| (s, ()))
}

fn alpha_just_1(input: &str) -> Res<&str, char> {
    satisfy(|c| c == '_' || ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z'))(input)
}

fn alphanumeric_just_1(input: &str) -> Res<&str, char> {
    satisfy(|c| {
        c == '_' || ('0' <= c && c <= '9') || ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
    })(input)
}

fn symbol(input: &str) -> Res<&str, String> {
    tuple((alpha_just_1, many0(alphanumeric_just_1)))(input).map(|(s, (head, tail))| {
        let mut name: String = head.to_string();
        for c in tail.iter() {
            name += &c.to_string()
        }
        (s, name)
    })
}

fn method(input: &str) -> Res<&str, String> {
    tuple((symbol, char(':'), space0))(input).map(|(s, (sym, _, _))| (s, sym))
}

fn emptyarg(input: &str) -> Res<&str, Vec<String>> {
    tuple((char('('), space0, char(')')))(input).map(|(s, _)| (s, vec![]))
}

fn contentarg(input: &str) -> Res<&str, Vec<String>> {
    tuple((
        char('('),
        space0,
        symbol,
        space0,
        many0(tuple((char(','), space0, symbol, space0))),
        char(')'),
    ))(input)
    .map(|(s, (_, _, head, _, rest, _))| {
        let mut syms: Vec<String> = rest.into_iter().map(|(_, _, val, _)| val).collect();
        syms.insert(0, head);
        (s, syms)
    })
}

fn arg(input: &str) -> Res<&str, Vec<String>> {
    alt((emptyarg, contentarg))(input)
}

fn ret(input: &str) -> Res<&str, String> {
    tuple((tag("->"), space0, symbol))(input).map(|(s, (_, _, sym))| (s, sym))
}

fn fntype(input: &str) -> Res<&str, (Vec<String>, String)> {
    tuple((arg, space0, ret))(input).map(|(s, (arg, _, ret))| (s, (arg, ret)))
}

pub fn fn_def(input: &str) -> Res<&str, FuncDef> {
    tuple((def, space1, method, fntype))(input).map(|(s, (_, _, name, (argstype, rettype)))| {
        (
            s,
            FuncDef {
                name,
                argstype,
                rettype,
            },
        )
    })
}

pub fn parse(input: &str) -> Res<&str, Vec<FuncDef>> {
    tuple((
        multispace0,
        separated_list0(permutation((space0, many1(char('\n')), space0)), fn_def),
    ))(input)
    .map(|(s, (_, list))| (s, list))
}
