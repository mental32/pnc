use {
    crate::parsing::Rule,
    cranelift_codegen::ir::{
        immediates::Imm64,
        types::{B8, I64, I8},
        Inst, InstBuilder, Type, Value,
    },
    cranelift_frontend::{FunctionBuilder, Variable},
    cranelift_entity::EntityRef,
    pest::iterators::Pair,
    std::io,
};

pub type TypedValue = (Value, Type);

#[derive(Debug)]
pub enum CodeChange {
    BlockReturns(Vec<TypedValue>),
    Instr(Inst),
    Value(Value),
    TypedValue(TypedValue),
}

pub fn codegen(
    pair: Pair<Rule>,
    mut builder: &mut FunctionBuilder,
) -> io::Result<Option<CodeChange>> {
    let mut return_value: Option<CodeChange> = None;

    dbg!(&pair.as_str());

    match dbg!(pair.as_rule()) {
        Rule::file => {
            for inner in pair.into_inner() {
                return_value = codegen(inner, &mut builder)?;
            }
        }

        Rule::s_expr => {
            let block = builder.create_ebb();
            builder.ins().jump(block, &[]);
            builder.switch_to_block(block);

            for inner in pair.into_inner() {
                return_value = codegen(inner, &mut builder)?;
            }
        }

        Rule::atom => {
            return_value = codegen(pair.into_inner().last().unwrap(), &mut builder)?;
        }

        Rule::boolean => {
            let inner = pair.into_inner().last().unwrap();

            let bool_ = match inner.as_rule() {
                Rule::truth => builder.ins().bconst(B8, true),
                Rule::falsity => builder.ins().bconst(B8, false),
                _ => unreachable!("What the fuck kinda boolean literal is this?!"),
            };

            let int_bool = builder.ins().bint(I8, bool_);
            return_value = Some(CodeChange::TypedValue((int_bool, I8)));
        }

        Rule::number => {
            let raw_n: i64 = dbg!(pair.as_str().parse().unwrap());
            let encoded = builder.ins().iconst(I64, Imm64::new(raw_n));
            return_value = Some(CodeChange::TypedValue((encoded, I64)));
        }

        Rule::operation => {
            let mut inner = pair.clone().into_inner();
            let operator = dbg!(inner.next().unwrap().as_str());
            let mut rest = inner.collect::<Vec<Pair<Rule>>>();

            let atoms: Vec<Pair<Rule>> = rest.drain_filter(|pair| pair.as_rule() == Rule::atom).collect();
            let s_exprs: Vec<Pair<Rule>> = rest;

            let mut returns = vec![];

            // No S expressions means the operation is formed of only atoms
            // Out grammar already picks up empty brackets as boolean falsities.
            if s_exprs.len() == 0 {
                for atom in atoms {
                    match codegen(atom, &mut builder)? {
                        Some(CodeChange::TypedValue(tv)) => returns.push(tv),
                        _ => unreachable!(),
                    }
                }

                let retval = {
                    let zero = builder.ins().iconst(I64, 0);
                    let mut acc_value = zero;

                    for (value, _) in returns {
                        acc_value = match operator {
                            "+" => builder.ins().iadd(acc_value, value),
                            "-" => builder.ins().isub(acc_value, value),
                            "*" => builder.ins().imul(acc_value, value),
                            _ => unreachable!()
                        }
                    }

                    (acc_value, I64)
                };

                return Ok(Some(CodeChange::BlockReturns(vec![retval])));
            }
 
            for pair in s_exprs.into_iter().chain(atoms) {
                match codegen(pair, &mut builder)? {
                    Some(CodeChange::TypedValue(tv)) => returns.push(tv),
                    Some(CodeChange::BlockReturns(returns_)) => returns.extend(returns_),
                    value => unreachable!(format!("{:?}", value)),
                }
            }

            return_value = Some(CodeChange::BlockReturns(returns));
        }

        Rule::EOI => {}
        _ => unreachable!("You've gone and fucked it now have't you?"),
    }

    Ok(return_value)
}
