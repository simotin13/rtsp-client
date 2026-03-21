use std::io::{self, Write, Seek, SeekFrom};
use std::fs::File;

// ============================================================
// データ構造
// ============================================================

/// 1サンプル(フレーム)の情報
#[derive(Debug, Clone)]
struct SampleInfo {
    /// ファイル内の絶対オフセット（length-prefixの先頭）
    offset: u64,
    /// サンプルのバイト数（length-prefixの4バイトを含む）
    size: u32,
    /// RTPタイムスタンプ（90kHz基準）
    dts: u32,
    /// IDRフレームかどうか
    is_keyframe: bool,
}

/// MP4ファイルライター
///
/// # 使い方
/// ```
/// let file = File::create("output.mp4")?;
/// let mut mp4 = Mp4Writer::new(file, 1280, 720);
///
/// mp4.write_header()?;
/// mp4.set_sps_pps(sps, pps);
///
/// // フレームごとに呼ぶ
/// mp4.write_sample(&nal_data, rtp_timestamp, is_idr)?;
///
/// // 録画終了
/// mp4.finalize()?;
/// ```
pub struct Mp4Writer {
    /// 書き込み先（BufWriterなしのFileを直接保持）
    writer: File,
    /// 蓄積したサンプル情報
    samples: Vec<SampleInfo>,
    /// SPS（スタートコードなし）
    sps: Vec<u8>,
    /// PPS（スタートコードなし）
    pps: Vec<u8>,
    /// 映像の幅（ピクセル）
    width: u16,
    /// 映像の高さ（ピクセル）
    height: u16,
    /// タイムスケール（RTPと合わせて 90000 推奨）
    timescale: u32,
    /// mdatのサイズフィールドのファイル内位置
    mdat_size_pos: u64,
    /// mdatのデータ開始位置（サンプルオフセット計算用）
    mdat_data_start: u64,
    /// finalize() 済みフラグ（二重呼び出し防止）
    finalized: bool,
}

// ============================================================
// パブリックAPI
// ============================================================

impl Mp4Writer {
    /// 新しい Mp4Writer を作成する。
    ///
    /// # 引数
    /// * `file`   - 書き込み先のファイル（既に開いた File）
    /// * `width`  - 映像の幅（ピクセル）
    /// * `height` - 映像の高さ（ピクセル）
    pub fn new(file: File, width: u16, height: u16) -> Self {
        Mp4Writer {
            writer: file,
            samples: Vec::new(),
            sps: Vec::new(),
            pps: Vec::new(),
            width,
            height,
            timescale: 90000,
            mdat_size_pos: 0,
            mdat_data_start: 0,
            finalized: false,
        }
    }

    /// タイムスケールを変更する（デフォルト: 90000）。
    /// write_header() より前に呼ぶこと。
    pub fn set_timescale(&mut self, timescale: u32) {
        self.timescale = timescale;
    }

    /// SPS と PPS を設定する。
    /// スタートコード（00 00 00 01）を除いた生NALデータを渡すこと。
    /// write_header() より後、最初の write_sample() より前に呼ぶこと。
    pub fn set_sps_pps(&mut self, sps: Vec<u8>, pps: Vec<u8>) {
        self.sps = sps;
        self.pps = pps;
    }

    /// ftyp と mdat ヘッダを書き込む。録画開始時に1度だけ呼ぶ。
    pub fn write_header(&mut self) -> io::Result<()> {
        self.write_ftyp()?;
        self.mdat_size_pos = self.writer.stream_position()?;
        // サイズは finalize() で上書きするのでプレースホルダ
        self.writer.write_all(&0u32.to_be_bytes())?;
        self.writer.write_all(b"mdat")?;
        self.mdat_data_start = self.writer.stream_position()?;
        Ok(())
    }

    /// 1フレーム分のNALユニットを書き込む。
    ///
    /// # 引数
    /// * `nal`        - スタートコードなしの生NALデータ
    /// * `dts`        - RTPタイムスタンプ（90kHz基準）
    /// * `is_keyframe`- IDRフレームなら true
    pub fn write_sample(&mut self, nal: &[u8], dts: u32, is_keyframe: bool) -> io::Result<()> {
        let offset = self.writer.stream_position()?;
        let nal_size = nal.len() as u32;

        // length-prefix（4バイトBE）＋NALデータ
        self.writer.write_all(&nal_size.to_be_bytes())?;
        self.writer.write_all(nal)?;

        self.samples.push(SampleInfo {
            offset,
            size: nal_size + 4,
            dts,
            is_keyframe,
        });

        Ok(())
    }

    /// 書き込み済みサンプル数を返す。
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 録画を終了し、mdatサイズとmoovを書き込む。
    /// 呼び出し後はこのライターを使用しないこと。
    pub fn finalize(&mut self) -> io::Result<()> {
        if self.finalized {
            return Ok(());
        }
        self.finalized = true;

        // 1. mdat サイズを確定して書き戻す
        let end_pos = self.writer.stream_position()?;
        let mdat_size = (end_pos - self.mdat_size_pos) as u32;
        self.writer.seek(SeekFrom::Start(self.mdat_size_pos))?;
        self.writer.write_all(&mdat_size.to_be_bytes())?;
        self.writer.seek(SeekFrom::Start(end_pos))?;

        // 2. moov を書く
        self.write_moov()?;

        self.writer.flush()?;
        Ok(())
    }
}

impl Drop for Mp4Writer {
    /// finalize() を呼ばずに drop された場合でも可能な限り書き込みを確定する。
    fn drop(&mut self) {
        if !self.finalized && !self.samples.is_empty() {
            if let Err(e) = self.finalize() {
                eprintln!("Mp4Writer::drop: finalize failed: {}", e);
            }
        }
    }
}

// ============================================================
// Box書き込みヘルパー
// ============================================================

impl Mp4Writer {
    /// Boxのサイズを後書きするクロージャヘルパー。
    ///
    /// クロージャ実行後、先頭の size フィールドを実際のバイト数で上書きする。
    fn write_box<F>(&mut self, fourcc: &[u8; 4], f: F) -> io::Result<u32>
    where
        F: FnOnce(&mut Self) -> io::Result<()>,
    {
        let pos = self.writer.stream_position()?;
        self.writer.write_all(&0u32.to_be_bytes())?; // size placeholder
        self.writer.write_all(fourcc)?;
        f(self)?;
        let end = self.writer.stream_position()?;
        let size = (end - pos) as u32;
        self.writer.seek(SeekFrom::Start(pos))?;
        self.writer.write_all(&size.to_be_bytes())?;
        self.writer.seek(SeekFrom::Start(end))?;
        Ok(size)
    }

    /// 単位行列（QuickTime/MP4 の 3x3 変換行列）を書き込む。
    fn write_matrix(&mut self) -> io::Result<()> {
        self.writer.write_all(&[
            0x00, 0x01, 0x00, 0x00, // a  = 1.0 (16.16)
            0x00, 0x00, 0x00, 0x00, // b  = 0
            0x00, 0x00, 0x00, 0x00, // u  = 0   (2.30)
            0x00, 0x00, 0x00, 0x00, // c  = 0
            0x00, 0x01, 0x00, 0x00, // d  = 1.0
            0x00, 0x00, 0x00, 0x00, // v  = 0
            0x00, 0x00, 0x00, 0x00, // tx = 0
            0x00, 0x00, 0x00, 0x00, // ty = 0
            0x40, 0x00, 0x00, 0x00, // w  = 1.0 (2.30)
        ])
    }
}

// ============================================================
// ftyp
// ============================================================

impl Mp4Writer {
    fn write_ftyp(&mut self) -> io::Result<()> {
        self.writer.write_all(&32u32.to_be_bytes())?; // size = 4+4+4+4+(4*4) = 32
        self.writer.write_all(b"ftyp")?;
        self.writer.write_all(b"isom")?;               // major_brand
        self.writer.write_all(&0x00000200u32.to_be_bytes())?; // minor_version
        self.writer.write_all(b"isom")?;               // compatible_brands
        self.writer.write_all(b"iso2")?;
        self.writer.write_all(b"avc1")?;
        self.writer.write_all(b"mp41")?;
        Ok(())
    }
}

// ============================================================
// moov
// ============================================================

impl Mp4Writer {
    fn write_moov(&mut self) -> io::Result<()> {
        self.write_box(b"moov", |s| {
            s.write_mvhd()?;
            s.write_trak()?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_mvhd(&mut self) -> io::Result<()> {
        let duration_ms = self.calc_duration_ms();
        self.write_box(b"mvhd", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;          // version=0, flags=0
            s.writer.write_all(&0u32.to_be_bytes())?;          // creation_time
            s.writer.write_all(&0u32.to_be_bytes())?;          // modification_time
            s.writer.write_all(&1000u32.to_be_bytes())?;       // timescale = ms 単位
            s.writer.write_all(&duration_ms.to_be_bytes())?;   // duration
            s.writer.write_all(&0x00010000u32.to_be_bytes())?; // rate = 1.0
            s.writer.write_all(&0x0100u16.to_be_bytes())?;     // volume = 1.0
            s.writer.write_all(&[0u8; 10])?;                   // reserved
            s.write_matrix()?;
            s.writer.write_all(&[0u8; 24])?;                   // pre_defined
            s.writer.write_all(&2u32.to_be_bytes())?;          // next_track_id
            Ok(())
        })?;
        Ok(())
    }
}

// ============================================================
// trak
// ============================================================

impl Mp4Writer {
    fn write_trak(&mut self) -> io::Result<()> {
        self.write_box(b"trak", |s| {
            s.write_tkhd()?;
            s.write_mdia()?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_tkhd(&mut self) -> io::Result<()> {
        let duration_ms = self.calc_duration_ms();
        let width = self.width;
        let height = self.height;
        self.write_box(b"tkhd", |s| {
            // version=0, flags=3 (enabled | in_movie)
            s.writer.write_all(&3u32.to_be_bytes())?;
            s.writer.write_all(&0u32.to_be_bytes())?;         // creation_time
            s.writer.write_all(&0u32.to_be_bytes())?;         // modification_time
            s.writer.write_all(&1u32.to_be_bytes())?;         // track_id = 1
            s.writer.write_all(&0u32.to_be_bytes())?;         // reserved
            s.writer.write_all(&duration_ms.to_be_bytes())?;  // duration (mvhd と同じ timescale)
            s.writer.write_all(&[0u8; 8])?;                   // reserved
            s.writer.write_all(&0u16.to_be_bytes())?;         // layer
            s.writer.write_all(&0u16.to_be_bytes())?;         // alternate_group
            s.writer.write_all(&0u16.to_be_bytes())?;         // volume (映像なので 0)
            s.writer.write_all(&0u16.to_be_bytes())?;         // reserved
            s.write_matrix()?;
            // width / height : 16.16 固定小数点
            s.writer.write_all(&((width as u32) << 16).to_be_bytes())?;
            s.writer.write_all(&((height as u32) << 16).to_be_bytes())?;
            Ok(())
        })?;
        Ok(())
    }
}

// ============================================================
// mdia
// ============================================================

impl Mp4Writer {
    fn write_mdia(&mut self) -> io::Result<()> {
        self.write_box(b"mdia", |s| {
            s.write_mdhd()?;
            s.write_hdlr()?;
            s.write_minf()?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_mdhd(&mut self) -> io::Result<()> {
        let duration = self.calc_duration_ticks();
        let timescale = self.timescale;
        self.write_box(b"mdhd", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;         // version=0, flags=0
            s.writer.write_all(&0u32.to_be_bytes())?;         // creation_time
            s.writer.write_all(&0u32.to_be_bytes())?;         // modification_time
            s.writer.write_all(&timescale.to_be_bytes())?;    // timescale (90000)
            s.writer.write_all(&duration.to_be_bytes())?;     // duration (ticks)
            s.writer.write_all(&0x55C4u16.to_be_bytes())?;    // language = "und"
            s.writer.write_all(&0u16.to_be_bytes())?;         // pre_defined
            Ok(())
        })?;
        Ok(())
    }

    fn write_hdlr(&mut self) -> io::Result<()> {
        self.write_box(b"hdlr", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;  // version & flags
            s.writer.write_all(&0u32.to_be_bytes())?;  // pre_defined
            s.writer.write_all(b"vide")?;              // handler_type
            s.writer.write_all(&[0u8; 12])?;           // reserved
            s.writer.write_all(b"VideoHandler\0")?;    // name (null terminated)
            Ok(())
        })?;
        Ok(())
    }
}

// ============================================================
// minf
// ============================================================

impl Mp4Writer {
    fn write_minf(&mut self) -> io::Result<()> {
        self.write_box(b"minf", |s| {
            s.write_vmhd()?;
            s.write_dinf()?;
            s.write_stbl()?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_vmhd(&mut self) -> io::Result<()> {
        self.write_box(b"vmhd", |s| {
            s.writer.write_all(&1u32.to_be_bytes())?;  // flags=1 (規格上必須)
            s.writer.write_all(&0u64.to_be_bytes())?;  // graphicsMode + opcolor
            Ok(())
        })?;
        Ok(())
    }

    fn write_dinf(&mut self) -> io::Result<()> {
        self.write_box(b"dinf", |s| {
            s.write_box(b"dref", |s| {
                s.writer.write_all(&0u32.to_be_bytes())?;  // version & flags
                s.writer.write_all(&1u32.to_be_bytes())?;  // entry_count = 1
                // url box: self-contained（location フィールドなし、サイズ=12）
                s.writer.write_all(&12u32.to_be_bytes())?;
                s.writer.write_all(b"url ")?;
                s.writer.write_all(&1u32.to_be_bytes())?;  // flags=1 = self-contained
                Ok(())
            })?;
            Ok(())
        })?;
        Ok(())
    }
}

// ============================================================
// stbl
// ============================================================

impl Mp4Writer {
    fn write_stbl(&mut self) -> io::Result<()> {
        self.write_box(b"stbl", |s| {
            s.write_stsd()?;
            s.write_stts()?;
            s.write_stss()?;
            s.write_stsc()?;
            s.write_stsz()?;
            s.write_stco()?;
            Ok(())
        })?;
        Ok(())
    }

    // ----- stsd / avc1 / avcC -----

    fn write_stsd(&mut self) -> io::Result<()> {
        let width = self.width;
        let height = self.height;
        let sps = self.sps.clone();
        let pps = self.pps.clone();
        self.write_box(b"stsd", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;  // version & flags
            s.writer.write_all(&1u32.to_be_bytes())?;  // entry_count = 1
            s.write_avc1(width, height, &sps, &pps)?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_avc1(&mut self, width: u16, height: u16, sps: &[u8], pps: &[u8]) -> io::Result<()> {
        let sps = sps.to_vec();
        let pps = pps.to_vec();
        self.write_box(b"avc1", |s| {
            s.writer.write_all(&[0u8; 6])?;                            // reserved
            s.writer.write_all(&1u16.to_be_bytes())?;                  // data_reference_index
            s.writer.write_all(&[0u8; 16])?;                           // pre_defined + reserved
            s.writer.write_all(&width.to_be_bytes())?;                 // width
            s.writer.write_all(&height.to_be_bytes())?;                // height
            s.writer.write_all(&0x00480000u32.to_be_bytes())?;         // horiz_resolution 72dpi
            s.writer.write_all(&0x00480000u32.to_be_bytes())?;         // vert_resolution  72dpi
            s.writer.write_all(&0u32.to_be_bytes())?;                  // reserved
            s.writer.write_all(&1u16.to_be_bytes())?;                  // frame_count = 1
            s.writer.write_all(&[0u8; 32])?;                           // compressorname
            s.writer.write_all(&0x0018u16.to_be_bytes())?;             // depth = 24
            s.writer.write_all(&0xFFFFu16.to_be_bytes())?;             // pre_defined = -1
            s.write_avcc(&sps, &pps)?;
            Ok(())
        })?;
        Ok(())
    }

    fn write_avcc(&mut self, sps: &[u8], pps: &[u8]) -> io::Result<()> {
        assert!(!sps.is_empty(), "SPS must not be empty");
        assert!(!pps.is_empty(), "PPS must not be empty");
        self.write_box(b"avcC", |s| {
            s.writer.write_all(&[
                0x01,    // configurationVersion = 1
                sps[1],  // AVCProfileIndication
                sps[2],  // profile_compatibility
                sps[3],  // AVCLevelIndication
                0xFF,    // lengthSizeMinusOne = 3 → 4バイト length-prefix
                0xE1,    // numSequenceParameterSets = 1
            ])?;
            s.writer.write_all(&(sps.len() as u16).to_be_bytes())?;
            s.writer.write_all(sps)?;
            s.writer.write_all(&[0x01])?;  // numPictureParameterSets = 1
            s.writer.write_all(&(pps.len() as u16).to_be_bytes())?;
            s.writer.write_all(pps)?;
            Ok(())
        })?;
        Ok(())
    }

    // ----- stts -----

    /// DTS差分をランレングス圧縮して書く。
    fn write_stts(&mut self) -> io::Result<()> {
        let entries = self.build_stts_entries();
        self.write_box(b"stts", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;                  // version & flags
            s.writer.write_all(&(entries.len() as u32).to_be_bytes())?;
            for (count, delta) in &entries {
                s.writer.write_all(&count.to_be_bytes())?;
                s.writer.write_all(&delta.to_be_bytes())?;
            }
            Ok(())
        })?;
        Ok(())
    }

    fn build_stts_entries(&self) -> Vec<(u32, u32)> {
        let n = self.samples.len();
        if n == 0 {
            return vec![];
        }
        let mut entries: Vec<(u32, u32)> = Vec::new();
        for i in 0..n {
            let delta = if i + 1 < n {
                // 次フレームとのDTS差分
                self.samples[i + 1].dts.wrapping_sub(self.samples[i].dts)
            } else if n >= 2 {
                // 最終フレームは1つ前と同じdeltaを使う
                self.samples[n - 1].dts.wrapping_sub(self.samples[n - 2].dts)
            } else {
                // フレームが1枚だけ: 30fps想定のフォールバック
                3000
            };
            match entries.last_mut() {
                Some(last) if last.1 == delta => last.0 += 1,
                _ => entries.push((1, delta)),
            }
        }
        entries
    }

    // ----- stss -----

    /// キーフレーム（IDR）のサンプル番号（1-based）を書く。
    fn write_stss(&mut self) -> io::Result<()> {
        let keyframes: Vec<u32> = self.samples.iter()
            .enumerate()
            .filter(|(_, s)| s.is_keyframe)
            .map(|(i, _)| i as u32 + 1)
            .collect();
        self.write_box(b"stss", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;
            s.writer.write_all(&(keyframes.len() as u32).to_be_bytes())?;
            for idx in &keyframes {
                s.writer.write_all(&idx.to_be_bytes())?;
            }
            Ok(())
        })?;
        Ok(())
    }

    // ----- stsc -----

    /// 全サンプルを1チャンクにまとめる最小構成。
    fn write_stsc(&mut self) -> io::Result<()> {
        let sample_count = self.samples.len() as u32;
        let entry_count = if sample_count > 0 { 1u32 } else { 0u32 };
        self.write_box(b"stsc", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;            // version & flags
            s.writer.write_all(&entry_count.to_be_bytes())?;
            if entry_count > 0 {
                s.writer.write_all(&1u32.to_be_bytes())?;            // first_chunk = 1
                s.writer.write_all(&sample_count.to_be_bytes())?;    // samples_per_chunk
                s.writer.write_all(&1u32.to_be_bytes())?;            // sample_description_index = 1
            }
            Ok(())
        })?;
        Ok(())
    }

    // ----- stsz -----

    /// 各サンプルのバイト数を列挙する。
    fn write_stsz(&mut self) -> io::Result<()> {
        let sizes: Vec<u32> = self.samples.iter().map(|s| s.size).collect();
        self.write_box(b"stsz", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;            // version & flags
            s.writer.write_all(&0u32.to_be_bytes())?;            // sample_size = 0 (可変)
            s.writer.write_all(&(sizes.len() as u32).to_be_bytes())?;
            for size in &sizes {
                s.writer.write_all(&size.to_be_bytes())?;
            }
            Ok(())
        })?;
        Ok(())
    }

    // ----- stco -----

    /// チャンクのファイル内オフセットを書く。
    /// 1チャンク構成なので最初のサンプルのオフセットが唯一のエントリ。
    fn write_stco(&mut self) -> io::Result<()> {
        let entry_count = if self.samples.is_empty() { 0u32 } else { 1u32 };
        let first_offset = self.samples.first().map(|s| s.offset as u32).unwrap_or(0);
        self.write_box(b"stco", |s| {
            s.writer.write_all(&0u32.to_be_bytes())?;            // version & flags
            s.writer.write_all(&entry_count.to_be_bytes())?;
            if entry_count > 0 {
                s.writer.write_all(&first_offset.to_be_bytes())?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

// ============================================================
// タイムスタンプ計算ユーティリティ
// ============================================================

impl Mp4Writer {
    /// 総再生時間（timescale 単位）。
    /// 最終フレームのdeltaを加算して最終フレーム分の尺も含める。
    fn calc_duration_ticks(&self) -> u32 {
        let n = self.samples.len();
        if n == 0 {
            return 0;
        }
        let last_delta = if n >= 2 {
            self.samples[n - 1].dts.wrapping_sub(self.samples[n - 2].dts)
        } else {
            3000
        };
        self.samples[n - 1]
            .dts
            .wrapping_sub(self.samples[0].dts)
            .wrapping_add(last_delta)
    }

    /// 総再生時間（ミリ秒）。mvhd / tkhd の duration フィールド用。
    fn calc_duration_ms(&self) -> u32 {
        let ticks = self.calc_duration_ticks() as u64;
        (ticks * 1000 / self.timescale as u64) as u32
    }
}

