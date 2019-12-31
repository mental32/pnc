use {
    crate::parsing::Rule,
    cranelift_codegen::ir::{
        immediates::Imm64,
        types::{B8, I64, I8},
        InstBuilder, StackSlotData, StackSlotKind,
    },
    cranelift_frontend::FunctionBuilder,
    pest::iterators::Pair,
    std::io,
};

pub fn codegen(pair: Pair<Rule>, mut builder: &mut FunctionBuilder) -> io::Result<()> {
    match dbg!(pair.as_rule()) {
        Rule::file => {
            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::s_expr => {
            let block = builder.create_ebb();
            builder.ins().jump(block, &[]);
            builder.switch_to_block(block);

            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::atom => codegen(pair.into_inner().last().unwrap(), &mut builder)?,

        Rule::boolean => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 1);
            let slot = builder.create_stack_slot(data);

            let inner = pair.into_inner().last().unwrap();

            let bool_ = match inner.as_rule() {
                Rule::truth => builder.ins().bconst(B8, true),
                Rule::falsity => builder.ins().bconst(B8, false),
                _ => unreachable!("What the fuck kinda boolean literal is this?!"),
            };

            let int_bool = builder.ins().bint(I8, bool_);
            builder.ins().stack_store(int_bool, slot, 0);
        }

        Rule::number => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 8);
            let slot = builder.create_stack_slot(data);

            let raw_n: i64 = dbg!(pair.as_str().parse().unwrap());
            let encoded = builder.ins().iconst(I64, Imm64::new(raw_n));
            builder.ins().stack_store(encoded, slot, 0);
        }

        Rule::EOI => {}
        _ => unreachable!("You've gone and fucked it now have't you?"),
    }

    Ok(())
}
