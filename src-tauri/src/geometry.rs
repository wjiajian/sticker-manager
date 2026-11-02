//! 窗口坐标：macOS 使用左上原点的逻辑坐标，Windows 使用物理坐标和目标屏缩放。
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
pub fn restore<'a>(
    displays: &'a [Display],
    position: &BallPosition,
) -> Option<(&'a Display, Point)> {
    let display = displays
        .iter()
        .find(|d| d.id == position.display)
        .or_else(|| displays.iter().find(|d| d.primary))
        .or_else(|| displays.first())?;
    Some((display, edge_center(display, position.edge, position.y)))
}
pub fn edge_center(display: &Display, edge: Edge, y: f64) -> Point {
    let area = display.area;
    // 为 72px 拖入状态及外侧 12px 虚线圈保留完整空间。
    let pad = 52.0 * display.scale;
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
pub fn snap(display: &Display, center: Point) -> (BallPosition, Point) {
    let edge = if center.x < display.area.x + display.area.width / 2.0 {
        Edge::Left
    } else {
        Edge::Right
    };
    let y = ((center.y - display.area.y) / display.area.height).clamp(0.0, 1.0);
    let center = edge_center(display, edge, y);
    (
        BallPosition {
            display: display.id.clone(),
            edge,
            y: (center.y - display.area.y) / display.area.height,
        },
        center,
    )
}
pub fn beside_ball(display: &Display, center: Point, edge: Edge, width: f64, height: f64) -> Point {
    let scale = display.scale;
    let x = match edge {
        Edge::Left => center.x + 40.0 * scale,
        Edge::Right => center.x - 40.0 * scale - width * scale,
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
#[derive(Clone, Debug, PartialEq)]
pub struct BallFrame {
    pub rect: Rect,
    pub center: Point,
    pub tip_above: bool,
}
/// 窗口大小只随球的位置变化：悬停、拖入、提示和收起都不改窗口。改尺寸时网页要晚一两帧
/// 才按新偏移重画，球会闪到别处。
pub fn ball_frame(display: &Display, center: Point, edge: Edge) -> BallFrame {
    let s = display.scale;
    let width = 300.0;
    // 提示离球心 56，最多三行（56 高），上下各留 4。
    let height = 168.0;
    // 和 edge_center 的边距相同，贴边时窗口外沿正好在显示器边缘。
    let inset = 52.0;
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
pub fn collapsed(display: &Display, frame: &BallFrame, center: Point, edge: Edge) -> (Point, Rect) {
    let s = display.scale;
    let area = display.area;
    let x = if edge == Edge::Left {
        area.x
    } else {
        area.right()
    };
    (
        Point {
            x: (x - frame.rect.x) / s,
            y: frame.center.y,
        },
        Rect {
            x: if edge == Edge::Left {
                area.x
            } else {
                area.right() - 28.0 * s
            },
            y: center.y - 44.0 * s,
            width: 28.0 * s,
            height: 88.0 * s,
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
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
    fn snaps_to_nearest_side_and_clamps_height() {
        let d = display();
        let (p, c) = snap(&d, Point { x: 20.0, y: -20.0 });
        assert_eq!(p.edge, Edge::Left);
        assert_eq!(c, Point { x: 52.0, y: 82.0 });
        assert_eq!(
            snap(
                &d,
                Point {
                    x: 1300.0,
                    y: 500.0
                }
            )
            .0
            .edge,
            Edge::Right
        );
    }
    #[test]
    fn panel_is_inward_centered_and_clamped_at_both_ends() {
        let d = display();
        assert_eq!(
            beside_ball(
                &d,
                Point {
                    x: 1396.0,
                    y: 440.0
                },
                Edge::Right,
                360.0,
                524.0
            ),
            Point { x: 996.0, y: 178.0 }
        );
        assert_eq!(
            beside_ball(&d, Point { x: 52.0, y: 82.0 }, Edge::Left, 360.0, 524.0),
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
                524.0
            )
            .y,
            326.0
        );
    }
    #[test]
    fn missing_display_uses_primary_and_preserves_side_and_ratio() {
        let displays = [display()];
        let p = BallPosition {
            display: "unplugged".into(),
            edge: Edge::Left,
            y: 0.4,
        };
        let (d, c) = restore(&displays, &p).unwrap();
        assert_eq!(d.id, "main");
        assert_eq!(c, Point { x: 52.0, y: 358.0 });
    }
    #[test]
    fn collapsed_ball_sits_on_display_edge_and_window_never_spills() {
        let d = display();
        for edge in [Edge::Left, Edge::Right] {
            let c = edge_center(&d, edge, 0.4);
            let f = ball_frame(&d, c, edge);
            assert!(f.rect.x >= d.area.x && f.rect.right() <= d.area.right());
            let (center, visible) = collapsed(&d, &f, c, edge);
            assert_eq!(center.x, if edge == Edge::Left { 0.0 } else { 300.0 });
            assert_eq!(center.y, f.center.y);
            assert_eq!(visible.width, 28.0);
            assert!(visible.x >= d.area.x && visible.right() <= d.area.right());
        }
    }
    #[test]
    fn frame_keeps_ball_stationary_and_tip_inside_work_area() {
        let d = display();
        let c = edge_center(&d, Edge::Right, 1.0);
        let f = ball_frame(&d, c, Edge::Right);
        assert_eq!(f.rect.x + f.center.x, c.x);
        assert_eq!(f.rect.y + f.center.y, c.y);
        assert!(f.tip_above);
        assert!(f.rect.bottom() <= d.area.bottom());
        for edge in [Edge::Left, Edge::Right] {
            for y in [0.0, 0.4, 1.0] {
                let frame = ball_frame(&d, edge_center(&d, edge, y), edge);
                // 72px 球 + 两侧 12px 留白 + 2px 虚线均不能被裁切。
                assert!(frame.center.x >= 50.0 && frame.center.y >= 50.0);
                assert!(frame.center.x + 50.0 <= frame.rect.width);
                assert!(frame.center.y + 50.0 <= frame.rect.height);
                // 三行提示（56 高）离球心 56，不能被窗口裁掉。
                if frame.tip_above {
                    assert!(frame.center.y - 56.0 - 56.0 >= 0.0);
                } else {
                    assert!(frame.center.y + 56.0 + 56.0 <= frame.rect.height);
                }
            }
        }
    }
    #[test]
    fn secondary_negative_origin_and_scale_are_respected() {
        let mut d = display();
        d.id = "second".into();
        d.frame.x = -2880.0;
        d.area.x = -2880.0;
        d.scale = 2.0;
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
        let c = edge_center(&d, Edge::Left, 0.4);
        let f = ball_frame(&d, c, Edge::Left);
        assert_eq!(f.rect.width, 600.0);
        assert_eq!(f.rect.x, d.area.x);
        let (center, visible) = collapsed(&d, &f, c, Edge::Left);
        assert_eq!(center.x, 0.0);
        assert_eq!(visible.width, 56.0);
    }
}
