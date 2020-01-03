use {
    crate::parsing::Rule,
    cranelift_codegen::ir::{
        types::{B8, I16, I32, I64, I8},
        Inst, InstBuilder, Type, Value,
    },
    cranelift_frontend::FunctionBuilder,
    pest::iterators::Pair,
    std::io,
};

pub type TypedValue = (Value, Type);

fn parse(this: &str, rule: Rule) -> i64 {
    let raw = match rule {
        Rule::base10 => dbg!(this.parse::<i64>()),
        Rule::base16 => i64::from_str_radix(this.get(2..).unwrap(), 16),
        Rule::base8 => i64::from_str_radix(this.get(2..).unwrap(), 8),
        Rule::base2 => i64::from_str_radix(this.get(2..).unwrap(), 2),
        _ => unreachable!(),
    }
    .unwrap();

    raw
}

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

    match dbg!(pair.as_rule()) {
        Rule::file => {
            for inner in pair.into_inner() {
                return_value = codegen(inner, &mut builder)?;
            }
        }

        Rule::s_expr => {
            let mut collected: Vec<Pair<Rule>> = pair.into_inner().collect();

            if collected.len() == 1 && collected[0].as_rule() == Rule::atom {
                return Ok(codegen(collected.pop().unwrap(), &mut builder)?);
            }

            if !builder.is_pristine() {
                let block = builder.create_ebb();
                builder.ins().jump(block, &[]);
                builder.switch_to_block(block);
            }

            for inner in collected {
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
            let number = pair.into_inner().last().unwrap();

            let rule = number.as_rule();
            let span = number.as_str();

            let _signed = rule == Rule::base10 && span.starts_with("-");
            let raw = parse(span, rule);
            let bit_count = 64 - raw.leading_zeros();

            let tp = match bit_count {
                0..=8 => I8,
                9..=16 => I16,
                17..=32 => I32,
                33..=64 => I64,
                _ => unreachable!(),
            };

            let encoded = builder.ins().iconst(tp, raw);
            return_value = Some(CodeChange::TypedValue((encoded, tp)));
        }

        Rule::operation => {
            let mut inner = pair.clone().into_inner();
            let operator = inner.next().unwrap();

            let mut rest = inner
                .into_iter()
                .enumerate()
                .collect::<Vec<(usize, Pair<Rule>)>>();

            fn is_atomic(pair: &mut Pair<Rule>) -> bool {
                (pair.as_rule() == Rule::atom
                    || pair.clone().into_inner().last().unwrap().as_rule() == Rule::atom)
            }

            let atoms: Vec<(usize, Pair<Rule>)> =
                rest.drain_filter(|(_, pair)| is_atomic(pair)).collect();

            let s_exprs: Vec<(usize, Pair<Rule>)> = rest;

            // No S expressions means the operation is formed of only atoms
            // Our grammar already picks up empty brackets as boolean falsities.
            if s_exprs.len() == 0 {
                let mut returns: Vec<TypedValue> = vec![];

                for (_, atom) in atoms {
                    match codegen(atom, &mut builder)? {
                        Some(CodeChange::TypedValue(tv)) => returns.push(tv),
                        _ => unreachable!(),
                    }
                }

                let retval = {
                    let (mut acc_value, mut acc_tp) = returns
                        .pop()
                        .or_else(|| Some((builder.ins().iconst(I64, 0), I64)))
                        .unwrap();

                    for (mut value, tp) in returns {
                        if tp.bits() < acc_tp.bits() {
                            value = builder.ins().sextend(acc_tp, value);
                        } else if tp.bits() > acc_tp.bits() {
                            acc_value = builder.ins().sextend(tp, acc_value);
                            acc_tp = tp;
                        }

                        acc_value = match operator.as_str() {
                            "+" => builder.ins().iadd(acc_value, value),
                            "-" => builder.ins().isub(acc_value, value),
                            "*" => builder.ins().imul(acc_value, value),
                            _ => unreachable!(),
                        }
                    }

                    (acc_value, I64)
                };

                return Ok(Some(CodeChange::BlockReturns(vec![retval])));
            }

            let mut returns: Vec<(usize, TypedValue)> = vec![];
            for (index, pair) in s_exprs {
                match codegen(pair, &mut builder)? {
                    Some(CodeChange::BlockReturns(mut values)) => {
                        assert!(values.len() == 1);
                        //^ We should look into whether BlockReturns will ever return more than two SSA references
                        returns.push((index, values.pop().unwrap()));
                    }

                    Some(CodeChange::TypedValue(tv)) => returns.push((index, tv)),
                    value => unreachable!(format!("{:?}", value)),
                }
            }

            let mut arguments = vec![];
            let mut bucket: Vec<(usize, TypedValue)> = vec![];

            let block = if !builder.is_pristine() {
                builder.create_ebb()
            } else {
                builder.func.layout.last_ebb().unwrap()
            };

            for (index, (val, tp)) in &returns {
                let param = builder.append_ebb_param(block, *tp);
                arguments.push(*val);
                bucket.push((*index, (param, *tp)));
            }

            builder.ins().jump(block, arguments.as_slice());
            builder.switch_to_block(block);

            for (index, atom) in atoms {
                match codegen(atom, &mut builder)? {
                    Some(CodeChange::TypedValue(tv)) => bucket.push((index, tv)),
                    value => unreachable!(format!("{:?}", value)),
                }
            }

            bucket.sort_by_key(|v| v.0);
            bucket.reverse();

            let retval = {
                let mut acc_value = bucket
                    .pop()
                    .and_then(|v| Some((v.1).0))
                    .or_else(|| Some(builder.ins().iconst(I64, 0)))
                    .unwrap();

                for (_, (value, _)) in bucket.into_iter().rev() {
                    acc_value = match operator.as_str() {
                        "+" => builder.ins().iadd(acc_value, value),
                        "-" => builder.ins().isub(acc_value, value),
                        "*" => builder.ins().imul(acc_value, value),
                        _ => unreachable!(),
                    }
                }

                (acc_value, I64)
            };

            return_value = Some(CodeChange::BlockReturns(vec![retval]));
        }

        Rule::EOI => {}
        _ => unreachable!("You've gone and fucked it now have't you?"),
    }

    Ok(return_value)
}
