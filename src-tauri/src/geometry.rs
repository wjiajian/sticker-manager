//! 纯几何计算。调用方统一坐标系和缩放；预览在 macOS 上直接使用 AppKit 左下原点。
use crate::settings::{BallPosition, Edge};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub fn right(self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(self) -> f64 {
        self.y + self.height
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }
    pub fn clamp(self, p: Point, width: f64, height: f64) -> Point {
        Point {
            x: p.x.clamp(self.x, (self.right() - width).max(self.x)),
            y: p.y.clamp(self.y, (self.bottom() - height).max(self.y)),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub id: String,
    pub frame: Rect,
    pub area: Rect,
    pub scale: f64,
    pub primary: bool,
}
pub fn display_at(displays: &[Display], point: Point) -> Option<&Display> {
    displays
        .iter()
        .find(|d| d.frame.contains(point))
        .or_else(|| {
            displays.iter().min_by(|a, b| {
                let distance = |d: &Display| {
                    let p = d.frame.clamp(point, 0.0, 0.0);
                    (p.x - point.x).powi(2) + (p.y - point.y).powi(2)
                };
                distance(a).total_cmp(&distance(b))
            })
        })
}
pub const BALL_SIZE_MIN: u32 = 40;
pub const BALL_SIZE_MAX: u32 = 80;
pub const BALL_SIZE_DEFAULT: u32 = 56;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallMetrics {
    pub size: u32,
    pub drop_size: u32,
    pub inset: f64,
    pub hit_radius: f64,
    pub drop_hit_radius: f64,
    pub fast_radius: f64,
    pub collapsed_width: f64,
    pub collapsed_half_height: f64,
    pub panel_offset: f64,
    pub tip_offset: f64,
    pub frame_width: f64,
    pub frame_height: f64,
}

impl BallMetrics {
    pub fn new(size: u32) -> Self {
        let size = size.clamp(BALL_SIZE_MIN, BALL_SIZE_MAX);
        let drop_size = size + 16;
        let radius = f64::from(size) / 2.0;
        let drop_radius = f64::from(drop_size) / 2.0;
        // 为拖入态球体及外侧 12px 虚线圈、2px 边框和 2px 余量保留完整空间。
        let inset = drop_radius + 16.0;
        let tip_offset = drop_radius + 20.0;
        Self {
            size,
            drop_size,
            inset,
            // 常规命中半径同时覆盖 1.1× 悬停放大态（80px 时 40 * 1.1 = 44）。
            hit_radius: radius + 4.0,
            drop_hit_radius: drop_radius + 14.0,
            fast_radius: radius + 132.0,
            collapsed_width: radius,
            collapsed_half_height: radius + 16.0,
            // 面板距球边缘固定 12px。
            panel_offset: radius + 12.0,
            tip_offset,
            frame_width: 300.0,
            // 提示离球心 tip_offset，最多三行（56 高），上下各留 4。
            frame_height: inset + tip_offset + 60.0,
        }
    }
}

pub fn restore<'a>(
    displays: &'a [Display],
    position: &BallPosition,
    metrics: BallMetrics,
) -> Option<(&'a Display, Point)> {
    let display = displays
        .iter()
        .find(|d| d.id == position.display)
        .or_else(|| displays.iter().find(|d| d.primary))
        .or_else(|| displays.first())?;
    Some((
        display,
        edge_center(display, position.edge, position.y, metrics),
    ))
}
pub fn edge_center(display: &Display, edge: Edge, y: f64, metrics: BallMetrics) -> Point {
    let area = display.area;
    let pad = metrics.inset * display.scale;
    Point {
        x: match edge {
            Edge::Left => area.x + pad,
            Edge::Right => area.right() - pad,
        },
        y: (area.y + y * area.height).clamp(
            area.y + pad.min(area.height / 2.0),
            area.bottom() - pad.min(area.height / 2.0),
        ),
    }
}
pub fn snap(display: &Display, center: Point, metrics: BallMetrics) -> (BallPosition, Point) {
    let edge = if center.x < display.area.x + display.area.width / 2.0 {
        Edge::Left
    } else {
        Edge::Right
    };
    let y = ((center.y - display.area.y) / display.area.height).clamp(0.0, 1.0);
    let center = edge_center(display, edge, y, metrics);
    (
        BallPosition {
            display: display.id.clone(),
            edge,
            y: (center.y - display.area.y) / display.area.height,
        },
        center,
    )
}
pub fn beside_ball(
    display: &Display,
    center: Point,
    edge: Edge,
    width: f64,
    height: f64,
    metrics: BallMetrics,
) -> Point {
    let scale = display.scale;
    let x = match edge {
        Edge::Left => center.x + metrics.panel_offset * scale,
        Edge::Right => center.x - metrics.panel_offset * scale - width * scale,
    };
    display.area.clamp(
        Point {
            x,
            y: center.y - height * scale / 2.0,
        },
        width * scale,
        height * scale,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub enum PanelOrigin {
    #[default]
    Centered,
    Ball(Edge),
}

/// 返回卡片（不含透明阴影边距）的原点；y 轴方向由调用方决定。
pub fn preview_position(
    area: Rect,
    panel: Rect,
    origin: PanelOrigin,
    tile_center_y: f64,
    card_height: f64,
) -> Option<Point> {
    const WIDTH: f64 = 168.0;
    const GAP: f64 = 16.0;
    if card_height > area.height {
        return None;
    }
    let left = panel.x - GAP - WIDTH;
    let right = panel.right() + GAP;
    let candidates = match origin {
        PanelOrigin::Ball(Edge::Left) => [right, left],
        PanelOrigin::Centered | PanelOrigin::Ball(Edge::Right) => [left, right],
    };
    let x = candidates
        .into_iter()
        .find(|x| *x >= area.x && *x + WIDTH <= area.right())?;
    Some(Point {
        x,
        y: (tile_center_y - card_height / 2.0).clamp(area.y, area.bottom() - card_height),
    })
}
#[derive(Clone, Debug, PartialEq)]
pub struct BallFrame {
    pub rect: Rect,
    pub center: Point,
    pub tip_above: bool,
}
/// 窗口大小只随球的位置和设置的大小变化：悬停、拖入、提示和收起都不改窗口。
/// 改尺寸时网页要晚一两帧才按新偏移重画，球会闪到别处。
pub fn ball_frame(display: &Display, center: Point, edge: Edge, metrics: BallMetrics) -> BallFrame {
    let s = display.scale;
    let width = metrics.frame_width;
    let height = metrics.frame_height;
    // 和 edge_center 的边距相同，贴边时窗口外沿正好在显示器边缘。
    let inset = metrics.inset;
    let above = center.y + (height - inset) * s > display.area.bottom();
    let origin = display.area.clamp(
        Point {
            x: if edge == Edge::Right {
                center.x - (width - inset) * s
            } else {
                center.x - inset * s
            },
            y: center.y
                - if above {
                    (height - inset) * s
                } else {
                    inset * s
                },
        },
        width * s,
        height * s,
    );
    BallFrame {
        rect: Rect {
            x: origin.x,
            y: origin.y,
            width: width * s,
            height: height * s,
        },
        center: Point {
            x: (center.x - origin.x) / s,
            y: (center.y - origin.y) / s,
        },
        tip_above: above,
    }
}
/// 收起时球心移到显示器边缘，只露出一半，窗口不动。返回窗口内的球心，以及露出部分在
/// 屏幕上的悬停区域。
pub fn collapsed(
    display: &Display,
    frame: &BallFrame,
    center: Point,
    edge: Edge,
    metrics: BallMetrics,
) -> (Point, Rect) {
    let s = display.scale;
    let area = display.area;
    let x = if edge == Edge::Left {
        area.x
    } else {
        area.right()
    };
    let width = metrics.collapsed_width * s;
    let half_height = metrics.collapsed_half_height * s;
    (
        Point {
            x: (x - frame.rect.x) / s,
            y: frame.center.y,
        },
        Rect {
            x: if edge == Edge::Left {
                area.x
            } else {
                area.right() - width
            },
            y: center.y - half_height,
            width,
            height: half_height * 2.0,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_chooses_side_from_panel_origin() {
        let area = Rect {
            x: 0.0,
            y: 24.0,
            width: 1440.0,
            height: 820.0,
        };
        let panel = Rect {
            x: 540.0,
            y: 150.0,
            width: 360.0,
            height: 524.0,
        };
        for (origin, x) in [
            (PanelOrigin::Centered, 356.0),
            (PanelOrigin::Ball(Edge::Right), 356.0),
            (PanelOrigin::Ball(Edge::Left), 916.0),
        ] {
            assert_eq!(
                preview_position(area, panel, origin, 400.0, 216.0),
                Some(Point { x, y: 292.0 })
            );
        }
    }

    #[test]
    fn preview_switches_side_or_stays_hidden_when_neither_fits() {
        let area = Rect {
            x: 0.0,
            y: 24.0,
            width: 800.0,
            height: 820.0,
        };
        let panel = Rect {
            x: 40.0,
            y: 150.0,
            width: 360.0,
            height: 524.0,
        };
        assert_eq!(
            preview_position(area, panel, PanelOrigin::Centered, 400.0, 216.0)
                .unwrap()
                .x,
            416.0
        );
        let right = Rect { x: 400.0, ..panel };
        assert_eq!(
            preview_position(area, right, PanelOrigin::Ball(Edge::Left), 400.0, 216.0)
                .unwrap()
                .x,
            216.0
        );
        assert_eq!(
            preview_position(
                Rect {
                    width: 500.0,
                    ..area
                },
                panel,
                PanelOrigin::Centered,
                400.0,
                216.0
            ),
            None
        );
    }

    #[test]
    fn preview_clamps_vertically_and_supports_negative_monitor_origins() {
        let area = Rect {
            x: -1600.0,
            y: -900.0,
            width: 1600.0,
            height: 850.0,
        };
        let panel = Rect {
            x: -1000.0,
            y: -700.0,
            width: 360.0,
            height: 524.0,
        };
        for (center, y) in [(-880.0, -900.0), (-500.0, -616.0), (-60.0, -282.0)] {
            assert_eq!(
                preview_position(area, panel, PanelOrigin::Centered, center, 232.0),
                Some(Point { x: -1184.0, y })
            );
        }
        assert_eq!(
            preview_position(
                Rect {
                    height: 200.0,
                    ..area
                },
                panel,
                PanelOrigin::Centered,
                -800.0,
                232.0
            ),
            None
        );
    }
    fn display() -> Display {
        Display {
            id: "main".into(),
            frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 1440.0,
                height: 900.0,
            },
            area: Rect {
                x: 0.0,
                y: 30.0,
                width: 1440.0,
                height: 820.0,
            },
            scale: 1.0,
            primary: true,
        }
    }
    #[test]
    fn default_ball_metrics_match_baseline_and_clamp_out_of_range() {
        let m = BallMetrics::new(BALL_SIZE_DEFAULT);
        assert_eq!(m.size, 56);
        assert_eq!(m.drop_size, 72);
        assert_eq!(m.inset, 52.0);
        assert_eq!(m.hit_radius, 32.0);
        assert_eq!(m.drop_hit_radius, 50.0);
        assert_eq!(m.fast_radius, 160.0);
        assert_eq!(m.collapsed_width, 28.0);
        assert_eq!(m.collapsed_half_height, 44.0);
        assert_eq!(m.panel_offset, 40.0);
        assert_eq!(m.tip_offset, 56.0);
        assert_eq!(m.frame_width, 300.0);
        assert_eq!(m.frame_height, 168.0);
        assert_eq!(BallMetrics::new(0).size, BALL_SIZE_MIN);
        assert_eq!(BallMetrics::new(999).size, BALL_SIZE_MAX);
    }
    #[test]
    fn snaps_to_nearest_side_and_clamps_height() {
        let d = display();
        let m = BallMetrics::new(BALL_SIZE_DEFAULT);
        let (p, c) = snap(&d, Point { x: 20.0, y: -20.0 }, m);
        assert_eq!(p.edge, Edge::Left);
        assert_eq!(c, Point { x: 52.0, y: 82.0 });
        assert_eq!(
            snap(
                &d,
                Point {
                    x: 1300.0,
                    y: 500.0
                },
                m,
            )
            .0
            .edge,
            Edge::Right
        );
    }
    #[test]
    fn panel_is_inward_centered_and_clamped_at_both_ends() {
        let d = display();
        let m = BallMetrics::new(BALL_SIZE_DEFAULT);
        assert_eq!(
            beside_ball(
                &d,
                Point {
                    x: 1396.0,
                    y: 440.0
                },
                Edge::Right,
                360.0,
                524.0,
                m,
            ),
            Point { x: 996.0, y: 178.0 }
        );
        assert_eq!(
            beside_ball(&d, Point { x: 52.0, y: 82.0 }, Edge::Left, 360.0, 524.0, m),
            Point { x: 92.0, y: 30.0 }
        );
        assert_eq!(
            beside_ball(
                &d,
                Point {
                    x: 1396.0,
                    y: 806.0
                },
                Edge::Right,
                360.0,
                524.0,
                m,
            )
            .y,
            326.0
        );
        // 不同球径下，面板边缘到球边缘的水平间距始终为 12px。
        for size in [BALL_SIZE_MIN, BALL_SIZE_DEFAULT, BALL_SIZE_MAX] {
            let m = BallMetrics::new(size);
            let radius = f64::from(m.size) / 2.0;
            let right_center = edge_center(&d, Edge::Right, 0.4, m);
            let panel_left = beside_ball(&d, right_center, Edge::Right, 360.0, 524.0, m);
            assert_eq!(right_center.x - radius - (panel_left.x + 360.0), 12.0);
            let left_center = edge_center(&d, Edge::Left, 0.4, m);
            let panel_right = beside_ball(&d, left_center, Edge::Left, 360.0, 524.0, m);
            assert_eq!(panel_right.x - (left_center.x + radius), 12.0);
        }
    }
    #[test]
    fn missing_display_uses_primary_and_preserves_side_and_ratio() {
        let displays = [display()];
        let m = BallMetrics::new(BALL_SIZE_DEFAULT);
        let p = BallPosition {
            display: "unplugged".into(),
            edge: Edge::Left,
            y: 0.4,
        };
        let (d, c) = restore(&displays, &p, m).unwrap();
        assert_eq!(d.id, "main");
        assert_eq!(c, Point { x: 52.0, y: 358.0 });
    }
    #[test]
    fn collapsed_ball_sits_on_display_edge_and_window_never_spills() {
        let d = display();
        for size in [BALL_SIZE_MIN, BALL_SIZE_DEFAULT, BALL_SIZE_MAX] {
            let m = BallMetrics::new(size);
            for edge in [Edge::Left, Edge::Right] {
                let c = edge_center(&d, edge, 0.4, m);
                let f = ball_frame(&d, c, edge, m);
                assert!(f.rect.x >= d.area.x && f.rect.right() <= d.area.right());
                let (center, visible) = collapsed(&d, &f, c, edge, m);
                assert_eq!(
                    center.x,
                    if edge == Edge::Left {
                        0.0
                    } else {
                        m.frame_width
                    }
                );
                assert_eq!(center.y, f.center.y);
                assert_eq!(visible.width, m.collapsed_width);
                assert_eq!(visible.height, m.collapsed_half_height * 2.0);
                assert!(visible.x >= d.area.x && visible.right() <= d.area.right());
            }
        }
    }
    #[test]
    fn frame_keeps_ball_stationary_and_tip_inside_work_area() {
        let d = display();
        for size in [BALL_SIZE_MIN, BALL_SIZE_DEFAULT, BALL_SIZE_MAX] {
            let m = BallMetrics::new(size);
            let c = edge_center(&d, Edge::Right, 1.0, m);
            let f = ball_frame(&d, c, Edge::Right, m);
            assert_eq!(f.rect.x + f.center.x, c.x);
            assert_eq!(f.rect.y + f.center.y, c.y);
            assert!(f.tip_above);
            assert!(f.rect.bottom() <= d.area.bottom());
            for edge in [Edge::Left, Edge::Right] {
                for y in [0.0, 0.4, 1.0] {
                    let frame = ball_frame(&d, edge_center(&d, edge, y, m), edge, m);
                    // 拖入态球体 + 两侧 12px 留白 + 2px 虚线均不能被裁切。
                    assert!(
                        frame.center.x >= m.drop_hit_radius && frame.center.y >= m.drop_hit_radius
                    );
                    assert!(frame.center.x + m.drop_hit_radius <= frame.rect.width);
                    assert!(frame.center.y + m.drop_hit_radius <= frame.rect.height);
                    // 三行提示（56 高）离球心 tip_offset，不能被窗口裁掉。
                    if frame.tip_above {
                        assert!(frame.center.y - m.tip_offset - 56.0 >= 0.0);
                    } else {
                        assert!(frame.center.y + m.tip_offset + 56.0 <= frame.rect.height);
                    }
                }
            }
        }
    }
    #[test]
    fn secondary_negative_origin_and_scale_are_respected() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut d = display();
            d.id = "second".into();
            d.frame.x = -2880.0;
            d.area.x = -2880.0;
            d.scale = scale;
            let ds = [display(), d.clone()];
            assert_eq!(
                display_at(
                    &ds,
                    Point {
                        x: -2000.0,
                        y: 300.0
                    }
                )
                .unwrap()
                .id,
                "second"
            );
            for size in [BALL_SIZE_MIN, BALL_SIZE_DEFAULT, BALL_SIZE_MAX] {
                let m = BallMetrics::new(size);
                let c = edge_center(&d, Edge::Left, 0.4, m);
                let f = ball_frame(&d, c, Edge::Left, m);
                assert_eq!(f.rect.width, m.frame_width * scale);
                assert_eq!(f.rect.height, m.frame_height * scale);
                assert_eq!(f.rect.x, d.area.x);
                let (center, visible) = collapsed(&d, &f, c, Edge::Left, m);
                assert_eq!(center.x, 0.0);
                assert_eq!(visible.width, m.collapsed_width * scale);
            }
        }
    }
}
