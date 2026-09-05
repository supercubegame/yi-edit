//! CI 用的截图钩子。不设 `YI_EDIT_SHOT` 就完全不走这条路。
//!
//! 用**时间**而不是帧数做稳定判据：软件渲染下持续重绘的帧很便宜，
//! 按帧数会在字体图集还没上传前就触发，拍到一张空窗口——而那张图看起来很正常。

use std::path::PathBuf;

pub struct Shot {
    target: Option<PathBuf>,
    settle: f64,
    requested: bool,
    saved: bool,
}

impl Shot {
    pub fn from_env() -> Self {
        Self {
            target: std::env::var("YI_EDIT_SHOT").ok().map(PathBuf::from),
            settle: std::env::var("YI_EDIT_SHOT_SETTLE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.5),
            requested: false,
            saved: false,
        }
    }

    pub fn active(&self) -> bool {
        self.target.is_some()
    }

    /// 每帧调一次。稳定后请求截图，拿到图就写盘并关窗。
    pub fn tick(&mut self, ctx: &egui::Context) {
        let Some(path) = self.target.clone() else {
            return;
        };
        ctx.request_repaint();
        let t = ctx.input(|i| i.time);
        if !self.requested && t > self.settle {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
            return;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            let mut raw: Vec<u8> = Vec::with_capacity(w * h * 4);
            for px in &image.pixels {
                raw.extend_from_slice(&px.to_array());
            }
            match image::save_buffer(&path, &raw, w as u32, h as u32, image::ColorType::Rgba8) {
                Ok(()) => {
                    eprintln!("SHOT saved {} ({w}x{h})", path.display());
                    self.saved = true;
                }
                // 写不出去就要大声报：静默失败与「截图正常」在面板上一模一样。
                Err(e) => eprintln!("SHOT FAILED {}: {e}", path.display()),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub fn saved(&self) -> bool {
        self.saved
    }
}
