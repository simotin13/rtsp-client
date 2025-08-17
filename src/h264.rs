use colored::*;

pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize, // 読み込んだビット位置
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    // n ビット読む
    pub fn read_bits(&mut self, n: usize) -> Option<u32> {
        let mut val = 0u32;
        for _ in 0..n {
            let byte_idx = self.bit_pos / 8;
            if byte_idx >= self.data.len() { return None; }
            let bit_offset = 7 - (self.bit_pos % 8);
            let bit = (self.data[byte_idx] >> bit_offset) & 1;
            val = (val << 1) | bit as u32;
            self.bit_pos += 1;
        }
        Some(val)
    }

    // ue(v) 無符号 Exp-Golomb
    pub fn read_ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while let Some(bit) = self.read_bits(1) {
            if bit == 0 { zeros += 1; } else { break; }
        }
        let suffix = if zeros > 0 { self.read_bits(zeros)? } else { 0 };
        Some((1 << zeros) - 1 + suffix)
    }

    // se(v) 符号付き Exp-Golomb
    pub fn read_se(&mut self) -> Option<i32> {
        let code_num = self.read_ue()? as i32;
        Some(if code_num % 2 == 0 { -(code_num / 2) } else { (code_num + 1)/2 })
    }
}

fn scaling_list(br: &mut BitReader, list: &mut [u8], size: usize, use_default_flag: &mut bool) {
    // size 個分の値を読み込む処理
    for j in 0..size {
        // ここで各係数を読み込む
        // list[j] = br.read_ue().unwrap() as u8; など
    }
    // use_default_flag を必要に応じて更新
}
pub fn decode_sps(payload: &[u8]) {
    let mut br = BitReader::new(&payload[1..]);
     let profile_idc = br.read_bits(8).unwrap();
    match profile_idc {
        66 => {
            println!("SPS Profile({}): Baseline", profile_idc);
        },
        77 => {
            println!("SPS Profile({}): Main", profile_idc);
        },
        88 => {
            println!("SPS Profile({}): Extended", profile_idc);
        },
        100 => {
            println!("SPS Profile({}): High", profile_idc);
        }
        _ => {
            println!("SPS Profile({}): Unknown", profile_idc);
        }
    }
    let constraint_set0_flag = br.read_bits(1).unwrap() == 1;
    let constraint_set1_flag = br.read_bits(1).unwrap() == 1;
    let constraint_set2_flag = br.read_bits(1).unwrap() == 1;
    let constraint_set3_flag = br.read_bits(1).unwrap() == 1;
    let constraint_set4_flag = br.read_bits(1).unwrap() == 1;
    let constraint_set5_flag = br.read_bits(1).unwrap() == 1;
    let reserved_zero_4bits  = br.read_bits(1).unwrap() == 1;

    println!("{}", format!("SPS Constraint Set0 Flag: {}", constraint_set0_flag).red());
    println!("{}", format!("SPS Constraint Set1 Flag: {}", constraint_set1_flag).red());
    println!("{}", format!("SPS Constraint Set2 Flag: {}", constraint_set2_flag).red());
    println!("{}", format!("SPS Constraint Set3 Flag: {}", constraint_set3_flag).red());
    println!("{}", format!("SPS Constraint Set4 Flag: {}", constraint_set4_flag).red());
    println!("{}", format!("SPS Constraint Set5 Flag: {}", constraint_set5_flag).red());
    println!("{}", format!("SPS Reserved Zero 4 Bits: {}", reserved_zero_4bits).red());

    let level_idc = br.read_bits(8).unwrap();
    println!("{}", format!("level IDC : {}", level_idc).red());
    let sps_id = br.read_ue().unwrap();
    println!("{}", format!("SPS ID = {}", sps_id).red());

    let profile_idc_list = [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];
    if profile_idc_list.contains(&profile_idc) {
        let chroma_format_idc = br.read_ue().unwrap();
        println!("{}", format!("SPS chroma_format_idc = {}", sps_id).red());
        if chroma_format_idc == 3 {
            let separate_colour_plane_flag  = br.read_bits(1).unwrap() == 1;
             println!("{}", format!("SPS separate_colour_plane_flag: {}", separate_colour_plane_flag).red());
        }

        let bit_depth_luma_minus8 = br.read_ue().unwrap();
        println!("{}", format!("SPS bit_depth_luma_minus8 = {}", bit_depth_luma_minus8).red());
        let bit_depth_chroma_minus8 = br.read_ue().unwrap();
        println!("{}", format!("SPS bit_depth_chroma_minus8 = {}", bit_depth_chroma_minus8).red());
        let qpprime_y_zero_transform_bypass_flag = br.read_bits(1).unwrap() == 1;
        println!("{}", format!("SPS qpprime_y_zero_transform_bypass_flag = {}", qpprime_y_zero_transform_bypass_flag).red());
        let seq_scaling_matrix_present_flag = br.read_bits(1).unwrap() == 1;
        println!("{}", format!("SPS seq_scaling_matrix_present_flag = {}", seq_scaling_matrix_present_flag).red());
        if( seq_scaling_matrix_present_flag ) {
            let mut count = 12;
            if chroma_format_idc != 3 {
                count = 8;
            }
            for i in 0..count  {
                /*
                let seq_scaling_list_present_flag = br.read_bits(1).unwrap() == 1;
                if (seq_scaling_list_present_flag) {
                    if (i < 6) {
                        scaling_list( ScalingList4x4[ i ], 16, UseDefaultScalingMatrix4x4Flag[ i ] );
                    } else {
                       scaling_list( ScalingList8x8[ i − 6 ], 64, UseDefaultScalingMatrix8x8Flag[ i − 6 ] )
                    }
                }
                */
            }
        }
    }
}