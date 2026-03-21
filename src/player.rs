use std::sync::mpsc;
use std::thread;
use eframe::egui;
use openh264::decoder::Decoder;

// ============================================================
// GUI アプリ
// ============================================================

struct PlayerApp {
    /// バックグラウンドスレッドから RGB フレームを受け取るチャンネル
    rx: mpsc::Receiver<(Vec<u8>, usize, usize)>,
    /// 現在表示中のテクスチャ
    texture: Option<egui::TextureHandle>,
    /// フレームの幅・高さ（アスペクト比計算用）
    frame_size: [usize; 2],
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // チャンネルを全部読んで最新フレームだけ使う（遅延防止）
        let mut latest: Option<(Vec<u8>, usize, usize)> = None;
        while let Ok(frame) = self.rx.try_recv() {
            latest = Some(frame);
        }

        if let Some((rgb, w, h)) = latest {
            let image = egui::ColorImage::from_rgb([w, h], &rgb);
            self.texture = Some(ctx.load_texture(
                "rtsp_frame",
                image,
                egui::TextureOptions::default(),
            ));
            self.frame_size = [w, h];
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                if let Some(ref tex) = self.texture {
                    let [fw, fh] = [self.frame_size[0] as f32, self.frame_size[1] as f32];
                    let available = ui.available_size();
                    // アスペクト比を維持したまま最大サイズに合わせる
                    let scale = (available.x / fw).min(available.y / fh);
                    let display = egui::vec2(fw * scale, fh * scale);
                    // 上下左右中央に配置
                    let offset = (available - display) * 0.5;
                    ui.add_space(offset.y.max(0.0));
                    ui.horizontal(|ui| {
                        ui.add_space(offset.x.max(0.0));
                        ui.image((tex.id(), display));
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("ストリーム待機中...");
                    });
                }
            });

        // 新フレームが届いた際にすぐ再描画されるよう常にリペイントを要求
        ctx.request_repaint();
    }
}

// ============================================================
// エントリポイント
// ============================================================

pub fn run_player(rtsp_url: String) {
    // バックグラウンドスレッドとのチャンネル（バッファ 2 フレーム）
    // GUI が処理しきれない場合は古いフレームを捨てる
    let (tx, rx) = mpsc::sync_channel::<(Vec<u8>, usize, usize)>(2);

    thread::spawn(move || {
        rtp_decode_loop(rtsp_url, tx);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RTSP Player")
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "RTSP Player",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(PlayerApp {
                rx,
                texture: None,
                frame_size: [1280, 720],
            }))
        }),
    ) {
        eprintln!("eframe error: {}", e);
    }
}

// ============================================================
// RTP受信・デコードループ（バックグラウンドスレッド）
// ============================================================

fn rtp_decode_loop(rtsp_url: String, tx: mpsc::SyncSender<(Vec<u8>, usize, usize)>) {
    let api = openh264::OpenH264API::from_source();
    let mut decoder = match Decoder::new(api) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to create H264 decoder: {}", e);
            return;
        }
    };

    let rtp_receiver = crate::rtp::RTPReceiver::new();
    let rtp_port = rtp_receiver.get_rtp_port();
    println!("RTP port: {}", rtp_port);

    let mut client = match crate::rtsp_client::RTSPClient::new(rtsp_url, rtp_port) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect RTSP: {}", e);
            return;
        }
    };

    client.options().ok();
    client.describe().ok();
    client.setup_track1().ok();
    client.setup_track2().ok();
    client.play().ok();
    println!("RTSP PLAY sent, waiting for stream...");

    let mut fragment_buf: Vec<u8> = Vec::new();

    loop {
        let (_header, payload) = match rtp_receiver.receive() {
            Ok(r) => r,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                eprintln!("RTP receive error: {}", e);
                break;
            }
        };

        if payload.is_empty() {
            continue;
        }

        if let Some((rgb, w, h)) = decode_to_rgb(&mut decoder, &mut fragment_buf, &payload) {
            match tx.try_send((rgb, w, h)) {
                Ok(_) => {}
                // チャンネルが満杯 → フレームを捨てる（表示遅延防止）
                Err(mpsc::TrySendError::Full(_)) => {}
                // ウィンドウが閉じた → ループ終了
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
    }

    client.shutdown();
    println!("Player stopped.");
}

// ============================================================
// NALユニット → RGB 変換
// ============================================================

fn decode_to_rgb(
    decoder: &mut Decoder,
    fragment_buf: &mut Vec<u8>,
    payload: &[u8],
) -> Option<(Vec<u8>, usize, usize)> {
    let nal_header = payload[0];
    let nal_unit_type = nal_header & 0x1F;

    // デコーダに渡すデータを組み立てる
    let nal_data: Option<Vec<u8>> = match nal_unit_type {
        // Single NAL unit（SPS / PPS / IDR / Non-IDR）
        crate::rtp::NAL_UNIT_TYPE_SPS
        | crate::rtp::NAL_UNIT_TYPE_PPS
        | crate::rtp::NAL_UNIT_TYPE_IDR
        | crate::rtp::NAL_UNIT_TYPE_NON_IDR => {
            let mut d = vec![0x00, 0x00, 0x00, 0x01];
            d.extend_from_slice(payload);
            Some(d)
        }
        // FU-A（フラグメント）
        28 => {
            if payload.len() < 2 {
                return None;
            }
            let fu_header = payload[1];
            let start_bit      = (fu_header >> 7) & 0x01;
            let end_bit        = (fu_header >> 6) & 0x01;
            let fu_nal_unit_type = fu_header & 0x1F;
            let fu_nal_header   = (nal_header & 0xE0) | fu_nal_unit_type;

            if start_bit == 1 {
                fragment_buf.clear();
                fragment_buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, fu_nal_header]);
            }
            fragment_buf.extend_from_slice(&payload[2..]);

            if end_bit == 1 {
                Some(fragment_buf.clone())
            } else {
                None // まだ組み立て中
            }
        }
        _ => return None,
    };

    let nal_data = nal_data?;

    match decoder.decode(&nal_data) {
        Ok(Some(yuv)) => {
            let (w, h) = yuv.dimension_rgb();
            let mut rgb = vec![0u8; w * h * 3];
            yuv.write_rgb8(&mut rgb);
            Some((rgb, w, h))
        }
        _ => None,
    }
}
