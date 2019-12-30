use {
    cranelift_codegen::ir::{
        immediates::Imm64,
        types::{B8, I64, I8},
        InstBuilder, StackSlotData, StackSlotKind,
    },
    cranelift_frontend::{FunctionBuilder},
    pest::iterators::Pair,
};

pub fn codegen(pair: Pair<Rule>, mut builder: &mut FunctionBuilder) -> Result<(), ()> {
    match dbg!(pair.as_rule()) {
        Rule::file => {
            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::s_expr | Rule::atom => {
            let block = builder.create_ebb();
            builder.ins().jump(block, &[]);
            builder.switch_to_block(block);

            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::boolean => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 0);
            let slot = builder.create_stack_slot(data);

            {
                let false_ = builder.ins().bconst(B8, false);
                let int_false = builder.ins().bint(I8, false_);
                builder.ins().stack_store(int_false, slot, 0);
            }
        }

        Rule::number => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 0);
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

#[derive(Parser)]
#[grammar = "pnc.pest"]
pub struct Penance;
