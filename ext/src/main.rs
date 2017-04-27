extern crate parity_wasm;

use std::env;
use parity_wasm::{builder, elements};

type Insertion = (u32, u32, String);

pub fn update_call_index(opcodes: &mut elements::Opcodes, original_imports: usize, inserts: &[Insertion]) {
    use parity_wasm::elements::Opcode::*;
    for opcode in opcodes.elements_mut().iter_mut() {
        match opcode {
            &mut Block(_, ref mut block) | &mut If(_, ref mut block) | &mut Loop(_, ref mut block) => {
                update_call_index(block, original_imports, inserts)
            },
            &mut Call(ref mut call_index) => {
                if let Some(pos) = inserts.iter().position(|x| x.0 == *call_index) {
                    *call_index = (original_imports + pos) as u32; 
                } else if *call_index as usize > original_imports {
                    *call_index += inserts.len() as u32;
                }
            },
            _ => { }
        }
    }
}

fn main() {

    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        println!("Usage: {} input_file.wasm output_file.wasm", args[0]);
        return;
    }

    // Loading module
    let module = parity_wasm::deserialize_file(&args[1]).unwrap();

    let replaced_funcs = vec!["_free", "_malloc"];

    // Save import functions number for later
    let import_funcs_total = module
        .import_section().expect("Import section to exist")
        .entries()
        .iter()
        .filter(|e| if let &elements::External::Function(_) = e.external() { true } else { false })
        .count();

    // First, we find functions indices that are to be rewired to externals
    //   Triple is (function_index (callable), type_index, function_name)
    let replaces: Vec<Insertion> = replaced_funcs
        .into_iter()
        .filter_map(|f| {
            let export = module
                .export_section().expect("Export section to exist")
                .entries().iter()
                .find(|e| e.field() == f)
                .expect("All functions of interest to exist");

            if let &elements::Internal::Function(func_idx) = export.internal() {
                let type_ref = module
                    .functions_section().expect("Functions section to exist")
                    .entries()[func_idx as usize - import_funcs_total]
                    .type_ref();

                Some((func_idx, type_ref, export.field().to_owned()))
            } else {
                None
            }
        })
        .collect();

    // Second, we duplicate them as import definitions
    let mut mbuilder = builder::from_module(module);
    for &(_, type_ref, ref field) in replaces.iter() {
        mbuilder.push_import(
            builder::import()
                .module("env")
                .field(field)
                .external().func(type_ref)
                .build()
        );
    }

    // Back to mutable access
    let mut module = mbuilder.build();

    // Third, rewire all calls to imported functions and update all other calls indices
    for section in module.sections_mut() {
        match section {
            &mut elements::Section::Code(ref mut code_section) => {
                for ref mut func_body in code_section.bodies_mut() {
                    update_call_index(func_body.code_mut(), import_funcs_total, &replaces);
                }
            },
            &mut elements::Section::Export(ref mut export_section) => {
                for ref mut export in export_section.entries_mut() {
                    match export.internal_mut() {
                        &mut elements::Internal::Function(ref mut func_index) => {
                            if *func_index >= import_funcs_total as u32 { *func_index += replaces.len() as u32; }
                        },
                        _ => {}
                    } 
                }
            },            
            _ => { }
        }
    }

    // Forth step could be to eliminate now dead code in actual functions
    //   but it is managed by `wasm-opt`

    parity_wasm::serialize_to_file(&args[2], module).unwrap();    
}
