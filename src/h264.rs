use colored::*;

pub fn decode_sps(payload: &[u8]) {
    let mut pos = 1;
    let idc_profile = payload[pos];
    match idc_profile {
        66 => {
            println!("SPS Profile({}): Baseline", idc_profile);
        },
        77 => {
            println!("SPS Profile({}): Main", idc_profile);
        },
        88 => {
            println!("SPS Profile({}): Extended", idc_profile);
        },
        100 => {
            println!("SPS Profile({}): High", idc_profile);
        }
        _ => {
            println!("SPS Profile({}): Unknown", idc_profile);
        }
    }
    pos += 1;
    let tmp = payload[pos];
    let constraint_set0_flag = (0x80 & tmp) != 0;
    let constraint_set1_flag = (0x40 & tmp) != 0;
    let constraint_set2_flag = (0x20 & tmp) != 0;
    let constraint_set3_flag = (0x10 & tmp) != 0;
    let constraint_set4_flag = (0x08 & tmp) != 0;
    let constraint_set5_flag = (0x04 & tmp) != 0;
    let reserved_zero_4bits  = (0x03 & tmp) != 0;

    println!("{}", format!("SPS Constraint Set0 Flag: {}", constraint_set0_flag).red());
    println!("{}", format!("SPS Constraint Set1 Flag: {}", constraint_set1_flag).red());
    println!("{}", format!("SPS Constraint Set2 Flag: {}", constraint_set2_flag).red());
    println!("{}", format!("SPS Constraint Set3 Flag: {}", constraint_set3_flag).red());
    println!("{}", format!("SPS Constraint Set4 Flag: {}", constraint_set4_flag).red());
    println!("{}", format!("SPS Constraint Set5 Flag: {}", constraint_set5_flag).red());
    println!("{}", format!("SPS Reserved Zero 4 Bits: {}", reserved_zero_4bits).red());

    pos += 1;
    let level_idc = payload[pos];
    println!("{}", format!("level IDC : {}", level_idc).red());
    pos += 1;
}