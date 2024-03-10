pub fn parse_sps(payload: &[u8]) {
    let idc_profile = payload[1];
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
        _ => {
            println!("SPS Profile({}): Unknown", idc_profile);
        }
    }
    let tmp = payload[2];
    let constraint_set0_flag = 0x80 & tmp;
    let constraint_set1_flag = 0x40 & tmp;
    let constraint_set2_flag = 0x20 & tmp;
    let constraint_set3_flag = 0x10 & tmp;
    let reserved_zero_4bits = 0x0F & tmp;
    println!("SPS Constraint Set0 Flag: {}", constraint_set0_flag);
    println!("SPS Constraint Set1 Flag: {}", constraint_set1_flag);
    println!("SPS Constraint Set2 Flag: {}", constraint_set2_flag);
    println!("SPS Constraint Set3 Flag: {}", constraint_set3_flag);
    println!("SPS Reserved Zero 4 Bits: {}", reserved_zero_4bits);
}