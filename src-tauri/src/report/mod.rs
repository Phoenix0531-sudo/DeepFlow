//! P2：周报分享图 PNG（纯 image crate，无外部字体依赖）。

mod font5x7;

use crate::db::WeeklyReport;
use chrono::Local;
use font5x7::{draw_text, text_width, GlyphScale};
use image::{ImageBuffer, Rgb, RgbImage};
use std::path::{Path, PathBuf};

fn fill_rect(img: &mut RgbImage, x: i32, y: i32, w: i32, h: i32, c: Rgb<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(iw);
    let y1 = (y + h).min(ih);
    for yy in y0..y1 {
        for xx in x0..x1 {
            img.put_pixel(xx as u32, yy as u32, c);
        }
    }
}

fn fill_round_rect(img: &mut RgbImage, x: i32, y: i32, w: i32, h: i32, r: i32, c: Rgb<u8>) {
    // 简易圆角：四角挖掉距离圆心 > r 的像素
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(iw);
    let y1 = (y + h).min(ih);
    let r = r.max(0);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let in_corner = |cx: i32, cy: i32| -> bool {
                let dx = xx - cx;
                let dy = yy - cy;
                dx * dx + dy * dy > r * r
            };
            let cut = (xx < x + r && yy < y + r && in_corner(x + r, y + r))
                || (xx >= x + w - r && yy < y + r && in_corner(x + w - 1 - r, y + r))
                || (xx < x + r && yy >= y + h - r && in_corner(x + r, y + h - 1 - r))
                || (xx >= x + w - r && yy >= y + h - r && in_corner(x + w - 1 - r, y + h - 1 - r));
            if !cut {
                img.put_pixel(xx as u32, yy as u32, c);
            }
        }
    }
}

fn hline(img: &mut RgbImage, x: i32, y: i32, w: i32, c: Rgb<u8>) {
    fill_rect(img, x, y, w, 1, c);
}

/// 将周报渲染为 PNG，写入 `exports_dir`，返回完整路径。
pub fn export_weekly_png(report: &WeeklyReport, exports_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(exports_dir).map_err(|e| e.to_string())?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let path = exports_dir.join(format!("weekly-report-{stamp}.png"));

    const W: u32 = 720;
    const H: u32 = 960;
    let mut img: RgbImage = ImageBuffer::from_pixel(W, H, Rgb([11, 15, 20]));

    let bg_top = Rgb([18, 24, 32]);
    let panel = Rgb([22, 30, 40]);
    let border = Rgb([40, 50, 62]);
    let amber = Rgb([212, 160, 23]);
    let text = Rgb([232, 238, 246]);
    let muted = Rgb([139, 152, 168]);
    let ok = Rgb([62, 207, 142]);
    let danger = Rgb([232, 93, 93]);

    // 顶栏
    fill_rect(&mut img, 0, 0, W as i32, 120, bg_top);
    fill_rect(&mut img, 0, 120, W as i32, 4, amber);

    draw_text(&mut img, 40, 36, "DeepFlow", amber, GlyphScale::X3);
    draw_text(&mut img, 40, 78, "本周正向周报", text, GlyphScale::X2);

    let date_line = Local::now().format("%Y-%m-%d").to_string();
    let dw = text_width(&date_line, GlyphScale::X2);
    draw_text(
        &mut img,
        W as i32 - 40 - dw,
        78,
        &date_line,
        muted,
        GlyphScale::X2,
    );

    // 主指标卡
    fill_round_rect(&mut img, 36, 150, W as i32 - 72, 150, 16, panel);
    hline(&mut img, 36, 150, W as i32 - 72, border);
    draw_text(&mut img, 56, 170, "总专注时长", muted, GlyphScale::X2);
    let focus_str = format!("{} 分钟", report.total_focus_minutes);
    draw_text(&mut img, 56, 210, &focus_str, amber, GlyphScale::X4);

    let delta = report.vs_last_week_focus_delta_minutes;
    let (delta_c, delta_s) = if delta >= 0 {
        (ok, format!("较上周 +{delta} 分钟"))
    } else {
        (danger, format!("较上周 {delta} 分钟"))
    };
    draw_text(&mut img, 56, 268, &delta_s, delta_c, GlyphScale::X2);

    // 指标网格 2x3
    let metrics: [(&str, String, Rgb<u8>); 6] = [
        (
            "成功拉回",
            format!("{} 次", report.successful_pullbacks_count),
            ok,
        ),
        (
            "合规休息",
            format!("{} 分钟", report.total_borrowed_rest_minutes),
            text,
        ),
        (
            "平均专注",
            format!("{} 分/会话", report.avg_focus_minutes),
            text,
        ),
        (
            "中断相关",
            format!("{}", report.interrupted_count),
            if report.interrupted_count > 0 {
                danger
            } else {
                ok
            },
        ),
        (
            "黄金时段",
            report.golden_focus_hour_range.clone(),
            amber,
        ),
        ("态度", "正向记录 · 不羞辱".into(), muted),
    ];

    let card_w = 300;
    let card_h = 100;
    let gap = 20;
    let start_y = 330;
    for (i, (label, value, color)) in metrics.iter().enumerate() {
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        let x = 36 + col * (card_w + gap);
        let y = start_y + row * (card_h + gap);
        fill_round_rect(&mut img, x, y, card_w, card_h, 14, panel);
        draw_text(&mut img, x + 18, y + 22, label, muted, GlyphScale::X2);
        // 值过长则缩小
        let scale = if text_width(value, GlyphScale::X3) > card_w - 36 {
            GlyphScale::X2
        } else {
            GlyphScale::X3
        };
        draw_text(&mut img, x + 18, y + 54, value, *color, scale);
    }

    // 页脚
    fill_rect(&mut img, 0, H as i32 - 72, W as i32, 72, bg_top);
    draw_text(
        &mut img,
        40,
        H as i32 - 48,
        "DeepFlow · 本地专注 · 数据不出本机",
        muted,
        GlyphScale::X2,
    );

    img.save(&path).map_err(|e| format!("写 PNG 失败: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::WeeklyReport;
    use std::fs;

    fn sample_report() -> WeeklyReport {
        WeeklyReport {
            total_focus_minutes: 320,
            successful_pullbacks_count: 7,
            total_borrowed_rest_minutes: 48,
            golden_focus_hour_range: "20:00 - 22:00".into(),
            avg_focus_minutes: 40,
            interrupted_count: 1,
            vs_last_week_focus_delta_minutes: 35,
        }
    }

    fn tmp_exports() -> tempfile::TempDir {
        tempfile::tempdir().expect("tmp dir")
    }

    #[test]
    fn export_writes_png_with_timestamp_name() {
        let dir = tmp_exports();
        let path = export_weekly_png(&sample_report(), dir.path()).expect("export ok");
        assert!(path.exists(), "png file should exist");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("weekly-report-"),
            "name prefix: {name}"
        );
        assert!(name.ends_with(".png"), "name ext: {name}");
        // 时间戳 YYYYMMDD-HHMMSS 介于前缀与后缀之间，长度=15
        let stem = &name["weekly-report-".len()..name.len() - ".png".len()];
        assert_eq!(stem.len(), 15, "timestamp shape: {stem}");
        // YYYYMMDD-HHMMSS → dash at byte index 8
        assert_eq!(&stem.as_bytes()[8..9], b"-", "dash separator: {stem}");
        // 非空且为合法 PNG（magic bytes）
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.len() > 8, "png not empty");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    }

    #[test]
    fn export_creates_exports_dir_if_missing() {
        let dir = tmp_exports();
        let nested = dir.path().join("nested/exports");
        let path = export_weekly_png(&sample_report(), &nested).expect("export ok");
        assert!(path.exists());
    }

    #[test]
    fn export_unknown_chars_does_not_panic() {
        // 未收录的 CJK 字会走 missing() 回退（画□），不应 panic
        let mut r = sample_report();
        r.golden_focus_hour_range = "鲸龘贰".into();
        let dir = tmp_exports();
        let path = export_weekly_png(&r, dir.path()).expect("export ok");
        assert!(path.exists());
    }

    #[test]
    fn export_empty_report_renders_zeros() {
        let r = WeeklyReport {
            total_focus_minutes: 0,
            successful_pullbacks_count: 0,
            total_borrowed_rest_minutes: 0,
            golden_focus_hour_range: "暂无足够数据".into(),
            avg_focus_minutes: 0,
            interrupted_count: 0,
            vs_last_week_focus_delta_minutes: 0,
        };
        let dir = tmp_exports();
        let path = export_weekly_png(&r, dir.path()).expect("export ok");
        assert!(path.exists());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
}
