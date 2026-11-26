use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Vec2,
};

use super::config::{Theme, Wallpaper};

// Each palette keeps the same signal accent for active clicks while the
// surrounding surface changes: paper, graphite, sand, ocean, forest, contrast,
// rose, lavender, midnight, nord, coffee, matcha, glacier, apricot, mint,
// ember, plum or wine.
#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    glass: bool,
    bg: Color32,
    panel: Color32,
    input: Color32,
    line: Color32,
    line_soft: Color32,
    text: Color32,
    muted: Color32,
    dim: Color32,
    warning: Color32,
    danger: Color32,
    accent: Color32,
}

const PAPER: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(243, 241, 235),
    panel: Color32::from_rgb(250, 249, 245),
    input: Color32::from_rgb(236, 233, 226),
    line: Color32::from_rgb(210, 205, 195),
    line_soft: Color32::from_rgb(227, 223, 214),
    text: Color32::from_rgb(24, 23, 20),
    muted: Color32::from_rgb(106, 102, 94),
    dim: Color32::from_rgb(143, 137, 127),
    warning: Color32::from_rgb(164, 98, 15),
    danger: Color32::from_rgb(169, 46, 34),
    accent: Color32::from_rgb(209, 73, 44),
};

const GRAPHITE: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(24, 24, 22),
    panel: Color32::from_rgb(33, 33, 30),
    input: Color32::from_rgb(43, 42, 38),
    line: Color32::from_rgb(69, 67, 60),
    line_soft: Color32::from_rgb(50, 49, 44),
    text: Color32::from_rgb(238, 235, 226),
    muted: Color32::from_rgb(174, 168, 156),
    dim: Color32::from_rgb(126, 121, 112),
    warning: Color32::from_rgb(226, 166, 74),
    danger: Color32::from_rgb(234, 108, 94),
    accent: Color32::from_rgb(209, 73, 44),
};

const SAND: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(243, 235, 219),
    panel: Color32::from_rgb(250, 244, 232),
    input: Color32::from_rgb(235, 225, 205),
    line: Color32::from_rgb(216, 203, 179),
    line_soft: Color32::from_rgb(231, 220, 199),
    text: Color32::from_rgb(52, 42, 28),
    muted: Color32::from_rgb(124, 109, 89),
    dim: Color32::from_rgb(157, 144, 124),
    warning: Color32::from_rgb(171, 102, 18),
    danger: Color32::from_rgb(170, 52, 37),
    accent: Color32::from_rgb(184, 86, 46),
};

const OCEAN: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(17, 26, 36),
    panel: Color32::from_rgb(24, 35, 47),
    input: Color32::from_rgb(32, 46, 60),
    line: Color32::from_rgb(52, 70, 88),
    line_soft: Color32::from_rgb(37, 52, 67),
    text: Color32::from_rgb(226, 237, 245),
    muted: Color32::from_rgb(156, 177, 194),
    dim: Color32::from_rgb(110, 131, 148),
    warning: Color32::from_rgb(229, 172, 80),
    danger: Color32::from_rgb(238, 112, 104),
    accent: Color32::from_rgb(78, 178, 199),
};

const FOREST: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(19, 27, 22),
    panel: Color32::from_rgb(26, 36, 30),
    input: Color32::from_rgb(35, 48, 40),
    line: Color32::from_rgb(56, 74, 62),
    line_soft: Color32::from_rgb(41, 56, 47),
    text: Color32::from_rgb(228, 238, 229),
    muted: Color32::from_rgb(159, 177, 161),
    dim: Color32::from_rgb(112, 132, 115),
    warning: Color32::from_rgb(222, 169, 79),
    danger: Color32::from_rgb(234, 112, 96),
    accent: Color32::from_rgb(112, 180, 128),
};

const CONTRAST: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(255, 255, 255),
    panel: Color32::from_rgb(248, 248, 248),
    input: Color32::from_rgb(235, 235, 235),
    line: Color32::from_rgb(28, 28, 28),
    line_soft: Color32::from_rgb(160, 160, 160),
    text: Color32::from_rgb(0, 0, 0),
    muted: Color32::from_rgb(55, 55, 55),
    dim: Color32::from_rgb(90, 90, 90),
    warning: Color32::from_rgb(150, 80, 0),
    danger: Color32::from_rgb(190, 0, 0),
    accent: Color32::from_rgb(250, 204, 21),
};

const ROSE: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(250, 240, 241),
    panel: Color32::from_rgb(253, 247, 247),
    input: Color32::from_rgb(245, 229, 231),
    line: Color32::from_rgb(224, 199, 203),
    line_soft: Color32::from_rgb(239, 221, 224),
    text: Color32::from_rgb(59, 35, 39),
    muted: Color32::from_rgb(140, 106, 112),
    dim: Color32::from_rgb(179, 152, 157),
    warning: Color32::from_rgb(168, 106, 18),
    danger: Color32::from_rgb(179, 58, 72),
    accent: Color32::from_rgb(194, 85, 106),
};

const LAVENDER: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(243, 240, 250),
    panel: Color32::from_rgb(250, 248, 253),
    input: Color32::from_rgb(233, 228, 245),
    line: Color32::from_rgb(211, 203, 230),
    line_soft: Color32::from_rgb(228, 222, 241),
    text: Color32::from_rgb(46, 40, 66),
    muted: Color32::from_rgb(115, 107, 140),
    dim: Color32::from_rgb(162, 154, 184),
    warning: Color32::from_rgb(151, 107, 20),
    danger: Color32::from_rgb(176, 71, 90),
    accent: Color32::from_rgb(124, 107, 196),
};

const MIDNIGHT: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(14, 18, 32),
    panel: Color32::from_rgb(22, 28, 46),
    input: Color32::from_rgb(32, 40, 61),
    line: Color32::from_rgb(49, 59, 85),
    line_soft: Color32::from_rgb(35, 44, 66),
    text: Color32::from_rgb(230, 234, 245),
    muted: Color32::from_rgb(154, 165, 192),
    dim: Color32::from_rgb(107, 118, 145),
    warning: Color32::from_rgb(224, 177, 92),
    danger: Color32::from_rgb(239, 122, 130),
    accent: Color32::from_rgb(108, 140, 255),
};

const NORD: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(46, 52, 64),
    panel: Color32::from_rgb(53, 60, 74),
    input: Color32::from_rgb(59, 66, 82),
    line: Color32::from_rgb(76, 86, 106),
    line_soft: Color32::from_rgb(67, 76, 94),
    text: Color32::from_rgb(236, 239, 244),
    muted: Color32::from_rgb(170, 179, 197),
    dim: Color32::from_rgb(123, 134, 156),
    warning: Color32::from_rgb(235, 203, 139),
    danger: Color32::from_rgb(191, 97, 106),
    accent: Color32::from_rgb(136, 192, 208),
};

const COFFEE: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(244, 237, 228),
    panel: Color32::from_rgb(251, 246, 239),
    input: Color32::from_rgb(237, 225, 211),
    line: Color32::from_rgb(216, 198, 178),
    line_soft: Color32::from_rgb(232, 218, 203),
    text: Color32::from_rgb(58, 44, 32),
    muted: Color32::from_rgb(133, 111, 92),
    dim: Color32::from_rgb(176, 154, 133),
    warning: Color32::from_rgb(169, 107, 24),
    danger: Color32::from_rgb(180, 71, 47),
    accent: Color32::from_rgb(140, 90, 60),
};

const MATCHA: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(19, 26, 20),
    panel: Color32::from_rgb(27, 36, 28),
    input: Color32::from_rgb(37, 49, 38),
    line: Color32::from_rgb(55, 74, 57),
    line_soft: Color32::from_rgb(39, 53, 42),
    text: Color32::from_rgb(229, 239, 227),
    muted: Color32::from_rgb(159, 179, 157),
    dim: Color32::from_rgb(113, 133, 111),
    warning: Color32::from_rgb(217, 180, 91),
    danger: Color32::from_rgb(224, 122, 109),
    accent: Color32::from_rgb(141, 187, 106),
};

const GLACIER: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(239, 244, 248),
    panel: Color32::from_rgb(248, 251, 253),
    input: Color32::from_rgb(228, 236, 243),
    line: Color32::from_rgb(196, 209, 221),
    line_soft: Color32::from_rgb(216, 226, 235),
    text: Color32::from_rgb(20, 32, 43),
    muted: Color32::from_rgb(78, 96, 111),
    dim: Color32::from_rgb(128, 145, 160),
    warning: Color32::from_rgb(150, 92, 12),
    danger: Color32::from_rgb(176, 52, 52),
    accent: Color32::from_rgb(56, 127, 168),
};

const APRICOT: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(247, 239, 230),
    panel: Color32::from_rgb(252, 247, 241),
    input: Color32::from_rgb(240, 228, 214),
    line: Color32::from_rgb(219, 201, 182),
    line_soft: Color32::from_rgb(238, 226, 213),
    text: Color32::from_rgb(43, 31, 22),
    muted: Color32::from_rgb(111, 88, 68),
    dim: Color32::from_rgb(156, 133, 112),
    warning: Color32::from_rgb(152, 92, 12),
    danger: Color32::from_rgb(170, 58, 44),
    accent: Color32::from_rgb(192, 106, 43),
};

const MINT: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(238, 244, 238),
    panel: Color32::from_rgb(247, 251, 247),
    input: Color32::from_rgb(226, 236, 227),
    line: Color32::from_rgb(195, 214, 198),
    line_soft: Color32::from_rgb(218, 231, 220),
    text: Color32::from_rgb(22, 36, 26),
    muted: Color32::from_rgb(81, 104, 84),
    dim: Color32::from_rgb(134, 156, 137),
    warning: Color32::from_rgb(146, 95, 15),
    danger: Color32::from_rgb(170, 56, 56),
    accent: Color32::from_rgb(62, 138, 99),
};

const EMBER: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(30, 25, 22),
    panel: Color32::from_rgb(38, 32, 28),
    input: Color32::from_rgb(47, 40, 35),
    line: Color32::from_rgb(69, 58, 50),
    line_soft: Color32::from_rgb(52, 44, 38),
    text: Color32::from_rgb(242, 234, 228),
    muted: Color32::from_rgb(184, 169, 158),
    dim: Color32::from_rgb(138, 122, 110),
    warning: Color32::from_rgb(226, 176, 114),
    danger: Color32::from_rgb(209, 107, 96),
    accent: Color32::from_rgb(224, 139, 74),
};

const PLUM: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(32, 26, 38),
    panel: Color32::from_rgb(41, 33, 48),
    input: Color32::from_rgb(50, 40, 59),
    line: Color32::from_rgb(70, 58, 81),
    line_soft: Color32::from_rgb(54, 44, 63),
    text: Color32::from_rgb(239, 232, 245),
    muted: Color32::from_rgb(185, 169, 198),
    dim: Color32::from_rgb(139, 122, 153),
    warning: Color32::from_rgb(217, 176, 111),
    danger: Color32::from_rgb(212, 117, 138),
    accent: Color32::from_rgb(176, 127, 208),
};

const WINE: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(33, 22, 26),
    panel: Color32::from_rgb(43, 29, 34),
    input: Color32::from_rgb(52, 36, 42),
    line: Color32::from_rgb(73, 52, 59),
    line_soft: Color32::from_rgb(56, 39, 45),
    text: Color32::from_rgb(243, 232, 235),
    muted: Color32::from_rgb(190, 167, 173),
    dim: Color32::from_rgb(143, 118, 125),
    warning: Color32::from_rgb(222, 172, 102),
    danger: Color32::from_rgb(214, 100, 96),
    accent: Color32::from_rgb(184, 90, 108),
};

// A simulated frosted-glass theme. The backdrop is drawn by
// [`paint_backdrop`] and the cards float above it as translucent panes; no OS
// blur is involved, so it renders identically through the software backend.
const LIQUID_GLASS: Palette = Palette {
    dark: false,
    glass: true,
    bg: Color32::from_rgb(214, 232, 255),
    panel: Color32::from_rgb(250, 252, 255),
    input: Color32::from_rgb(255, 255, 255),
    // Lines are only a fallback: the glass theme derives its own dark
    // hairlines and light rims in [`line`], [`line_soft`] and [`card`].
    line: Color32::from_rgb(255, 255, 255),
    line_soft: Color32::from_rgb(255, 255, 255),
    text: Color32::from_rgb(29, 29, 31),
    muted: Color32::from_rgb(72, 72, 74),
    dim: Color32::from_rgb(110, 110, 115),
    warning: Color32::from_rgb(178, 80, 0),
    danger: Color32::from_rgb(215, 0, 21),
    accent: Color32::from_rgb(10, 132, 255),
};

/// Corner radii shared by the card frame, its specular edge and the dialog
/// pane so the painted rim can never drift away from the rounded fill.
const GLASS_CARD_RADIUS: u8 = 22;
const GLASS_DIALOG_RADIUS: u8 = 24;

// Seasonal and neon palettes. Halloween, Neon and Terminal Green are dark;
// Winter stays light so it feels like frost on a window.
const HALLOWEEN: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(20, 15, 26),
    panel: Color32::from_rgb(30, 22, 38),
    input: Color32::from_rgb(43, 32, 53),
    line: Color32::from_rgb(74, 54, 88),
    line_soft: Color32::from_rgb(55, 40, 66),
    text: Color32::from_rgb(244, 234, 216),
    muted: Color32::from_rgb(185, 166, 199),
    dim: Color32::from_rgb(133, 115, 148),
    warning: Color32::from_rgb(245, 163, 60),
    danger: Color32::from_rgb(255, 93, 93),
    accent: Color32::from_rgb(255, 122, 26),
};

const WINTER: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(234, 242, 251),
    panel: Color32::from_rgb(247, 251, 255),
    input: Color32::from_rgb(221, 234, 247),
    line: Color32::from_rgb(191, 213, 232),
    line_soft: Color32::from_rgb(214, 229, 242),
    text: Color32::from_rgb(23, 40, 58),
    muted: Color32::from_rgb(108, 129, 153),
    dim: Color32::from_rgb(157, 177, 198),
    warning: Color32::from_rgb(181, 124, 19),
    danger: Color32::from_rgb(200, 67, 62),
    accent: Color32::from_rgb(201, 154, 62),
};

const NEON: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(7, 7, 15),
    panel: Color32::from_rgb(15, 15, 28),
    input: Color32::from_rgb(24, 24, 41),
    line: Color32::from_rgb(46, 46, 77),
    line_soft: Color32::from_rgb(32, 32, 57),
    text: Color32::from_rgb(245, 243, 255),
    muted: Color32::from_rgb(169, 162, 221),
    dim: Color32::from_rgb(111, 104, 160),
    warning: Color32::from_rgb(255, 209, 102),
    danger: Color32::from_rgb(255, 77, 141),
    accent: Color32::from_rgb(34, 230, 255),
};

const TERMINAL_GREEN: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(5, 10, 6),
    panel: Color32::from_rgb(10, 20, 12),
    input: Color32::from_rgb(15, 30, 17),
    line: Color32::from_rgb(33, 69, 42),
    line_soft: Color32::from_rgb(23, 49, 30),
    text: Color32::from_rgb(201, 255, 210),
    muted: Color32::from_rgb(127, 201, 139),
    dim: Color32::from_rgb(78, 138, 90),
    warning: Color32::from_rgb(216, 255, 112),
    danger: Color32::from_rgb(255, 107, 107),
    accent: Color32::from_rgb(57, 255, 106),
};

// Seasonal wallpapers. These keep the normal opaque cards so the painted
// backdrop is the star of the show, just like Halloween and Winter.
const SPRING: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(251, 241, 246),
    panel: Color32::from_rgb(255, 249, 252),
    input: Color32::from_rgb(247, 228, 238),
    line: Color32::from_rgb(232, 198, 215),
    line_soft: Color32::from_rgb(242, 220, 230),
    text: Color32::from_rgb(59, 36, 49),
    muted: Color32::from_rgb(138, 98, 116),
    dim: Color32::from_rgb(184, 144, 160),
    warning: Color32::from_rgb(168, 100, 0),
    danger: Color32::from_rgb(192, 54, 77),
    accent: Color32::from_rgb(214, 76, 134),
};

const SUMMER: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(233, 246, 251),
    panel: Color32::from_rgb(248, 252, 254),
    input: Color32::from_rgb(221, 240, 247),
    line: Color32::from_rgb(183, 220, 232),
    line_soft: Color32::from_rgb(207, 232, 241),
    text: Color32::from_rgb(18, 52, 64),
    muted: Color32::from_rgb(77, 116, 128),
    dim: Color32::from_rgb(132, 166, 177),
    warning: Color32::from_rgb(169, 104, 0),
    danger: Color32::from_rgb(194, 58, 58),
    accent: Color32::from_rgb(14, 151, 190),
};

const AUTUMN: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(250, 239, 225),
    panel: Color32::from_rgb(255, 248, 239),
    input: Color32::from_rgb(244, 225, 203),
    line: Color32::from_rgb(226, 195, 160),
    line_soft: Color32::from_rgb(238, 215, 190),
    text: Color32::from_rgb(60, 39, 23),
    muted: Color32::from_rgb(135, 96, 63),
    dim: Color32::from_rgb(175, 138, 104),
    warning: Color32::from_rgb(154, 98, 0),
    danger: Color32::from_rgb(178, 58, 45),
    accent: Color32::from_rgb(185, 84, 28),
};

const VALENTINE: Palette = Palette {
    dark: false,
    glass: false,
    bg: Color32::from_rgb(252, 239, 243),
    panel: Color32::from_rgb(255, 248, 250),
    input: Color32::from_rgb(249, 223, 232),
    line: Color32::from_rgb(236, 192, 206),
    line_soft: Color32::from_rgb(244, 215, 224),
    text: Color32::from_rgb(66, 30, 43),
    muted: Color32::from_rgb(148, 95, 112),
    dim: Color32::from_rgb(190, 146, 161),
    warning: Color32::from_rgb(164, 90, 0),
    danger: Color32::from_rgb(188, 27, 59),
    accent: Color32::from_rgb(217, 45, 92),
};

const GALAXY: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(8, 11, 30),
    panel: Color32::from_rgb(17, 22, 46),
    input: Color32::from_rgb(26, 33, 64),
    line: Color32::from_rgb(42, 52, 92),
    line_soft: Color32::from_rgb(29, 37, 71),
    text: Color32::from_rgb(236, 239, 255),
    muted: Color32::from_rgb(166, 175, 207),
    dim: Color32::from_rgb(113, 123, 163),
    warning: Color32::from_rgb(255, 196, 92),
    danger: Color32::from_rgb(255, 107, 138),
    accent: Color32::from_rgb(138, 124, 255),
};

const AURORA: Palette = Palette {
    dark: true,
    glass: false,
    bg: Color32::from_rgb(6, 15, 21),
    panel: Color32::from_rgb(13, 27, 35),
    input: Color32::from_rgb(19, 40, 51),
    line: Color32::from_rgb(33, 64, 78),
    line_soft: Color32::from_rgb(24, 50, 61),
    text: Color32::from_rgb(230, 247, 245),
    muted: Color32::from_rgb(147, 183, 181),
    dim: Color32::from_rgb(95, 133, 133),
    warning: Color32::from_rgb(255, 209, 102),
    danger: Color32::from_rgb(255, 107, 122),
    accent: Color32::from_rgb(36, 201, 157),
};

const SANS_REGULAR: &str = "plex-sans-regular";
const SANS_MEDIUM: &str = "plex-sans-medium";
const SANS_SEMIBOLD: &str = "plex-sans-semibold";
const MONO_REGULAR: &str = "plex-mono-regular";
const MONO_MEDIUM: &str = "plex-mono-medium";

const FAMILY_SANS_MEDIUM: &str = "plex-sans-medium-family";
const FAMILY_SANS_SEMIBOLD: &str = "plex-sans-semibold-family";
const FAMILY_MONO_MEDIUM: &str = "plex-mono-medium-family";

static CURRENT: AtomicUsize = AtomicUsize::new(0);

pub fn install(ctx: &egui::Context, theme: Theme) {
    install_fonts(ctx);
    CURRENT.store(theme.index(), Ordering::Relaxed);
    install_style(ctx);
    ctx.set_theme(theme_preference(theme));
}

pub fn set_theme(ctx: &egui::Context, theme: Theme) {
    CURRENT.store(theme.index(), Ordering::Relaxed);
    install_style(ctx);
    ctx.set_theme(theme_preference(theme));
    ctx.request_repaint();
}

fn theme_preference(theme: Theme) -> egui::ThemePreference {
    if palette_for(theme).dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    }
}

fn palette_for(theme: Theme) -> Palette {
    match theme {
        Theme::Paper => PAPER,
        Theme::Graphite => GRAPHITE,
        Theme::Sand => SAND,
        Theme::Ocean => OCEAN,
        Theme::Forest => FOREST,
        Theme::Contrast => CONTRAST,
        Theme::Rose => ROSE,
        Theme::Lavender => LAVENDER,
        Theme::Midnight => MIDNIGHT,
        Theme::Nord => NORD,
        Theme::Coffee => COFFEE,
        Theme::Matcha => MATCHA,
        Theme::Glacier => GLACIER,
        Theme::Apricot => APRICOT,
        Theme::Mint => MINT,
        Theme::Ember => EMBER,
        Theme::Plum => PLUM,
        Theme::Wine => WINE,
        Theme::LiquidGlass => LIQUID_GLASS,
        Theme::Halloween => HALLOWEEN,
        Theme::Winter => WINTER,
        Theme::Neon => NEON,
        Theme::TerminalGreen => TERMINAL_GREEN,
        Theme::Spring => SPRING,
        Theme::Summer => SUMMER,
        Theme::Autumn => AUTUMN,
        Theme::Valentine => VALENTINE,
        Theme::Galaxy => GALAXY,
        Theme::Aurora => AURORA,
    }
}

fn palette() -> Palette {
    palette_for(current_theme())
}

pub fn bg() -> Color32 {
    palette().bg
}

pub fn panel() -> Color32 {
    palette().panel
}

pub fn input() -> Color32 {
    if palette().glass {
        alpha(Color32::WHITE, 118)
    } else {
        palette().input
    }
}

/// Slightly denser fill for editable fields and popup surfaces, where text
/// needs a calmer base than the surrounding card glass.
pub fn field() -> Color32 {
    if palette().glass {
        alpha(Color32::WHITE, 148)
    } else {
        palette().input
    }
}

pub fn line() -> Color32 {
    if palette().glass {
        // A dark hairline reads on the pale glass; white-on-white vanished.
        alpha(Color32::BLACK, 30)
    } else {
        palette().line
    }
}

pub fn line_soft() -> Color32 {
    if palette().glass {
        alpha(Color32::BLACK, 20)
    } else {
        palette().line_soft
    }
}

pub fn glass() -> bool {
    palette().glass
}

fn current_theme() -> Theme {
    Theme::from_index(CURRENT.load(Ordering::Relaxed))
}

fn decorated() -> bool {
    current_theme().wallpaper() != Wallpaper::None
}

pub fn text() -> Color32 {
    palette().text
}

pub fn muted() -> Color32 {
    palette().muted
}

pub fn dim() -> Color32 {
    palette().dim
}

pub fn accent() -> Color32 {
    palette().accent
}

/// Darker accent variant for small blue text on pale glass. The bright system
/// blue stays reserved for fills, glyphs and focus rings.
pub fn accent_ink() -> Color32 {
    if palette().glass {
        Color32::from_rgb(0, 102, 204)
    } else {
        palette().accent
    }
}

/// Readable foreground for content drawn on top of [`accent`].
pub fn on_accent() -> Color32 {
    let accent = palette().accent;
    let luma = 0.299 * accent.r() as f32 + 0.587 * accent.g() as f32 + 0.114 * accent.b() as f32;
    if luma > 150.0 {
        Color32::from_rgb(24, 23, 20)
    } else {
        Color32::from_rgb(252, 250, 245)
    }
}

pub fn warning() -> Color32 {
    palette().warning
}

pub fn danger() -> Color32 {
    palette().danger
}

/// Fill for the root central panel. In the glass theme it stays transparent
/// so [`paint_backdrop`] shows through; other themes keep the flat background.
pub fn backdrop_fill() -> Color32 {
    if decorated() {
        Color32::TRANSPARENT
    } else {
        palette().bg
    }
}

pub fn sans(size: f32) -> FontId {
    if current_theme() == Theme::TerminalGreen {
        FontId::new(size, FontFamily::Monospace)
    } else {
        FontId::new(size, FontFamily::Proportional)
    }
}

pub fn sans_medium(size: f32) -> FontId {
    if current_theme() == Theme::TerminalGreen {
        FontId::new(size, named_family(FAMILY_MONO_MEDIUM))
    } else {
        FontId::new(size, named_family(FAMILY_SANS_MEDIUM))
    }
}

pub fn sans_semibold(size: f32) -> FontId {
    if current_theme() == Theme::TerminalGreen {
        FontId::new(size, named_family(FAMILY_MONO_MEDIUM))
    } else {
        FontId::new(size, named_family(FAMILY_SANS_SEMIBOLD))
    }
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn mono_medium(size: f32) -> FontId {
    FontId::new(size, named_family(FAMILY_MONO_MEDIUM))
}

pub fn alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

pub fn shadow() -> Shadow {
    shadow_for(palette().dark)
}

pub fn rule(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0_f32, line_soft()),
    );
}

/// A soft rounded surface used to group related controls.
pub fn card() -> egui::Frame {
    if palette().glass {
        // No outer drop shadow: the footer lives in a bottom panel whose clip
        // rect would cut a rectangular slice out of a wide shadow. The pane
        // instead gets its depth from the rim and the inner bottom shade that
        // [`paint_glass_edge`] draws just inside the rounded rect.
        return egui::Frame::new()
            .fill(alpha(Color32::WHITE, 118))
            .stroke(Stroke::new(1.0_f32, alpha(Color32::WHITE, 195)))
            .corner_radius(CornerRadius::same(GLASS_CARD_RADIUS))
            .inner_margin(Margin::symmetric(14, 8));
    }

    egui::Frame::new()
        .fill(panel())
        .stroke(Stroke::new(1.0_f32, line_soft()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 8))
}

/// Draw a card and add the thin specular edge that makes the glass read as a
/// physical pane. Other themes fall back to [`card`] unchanged.
pub fn card_show<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let response = card().show(ui, add);
    if palette().glass {
        paint_glass_edge(ui.painter(), response.response.rect, GLASS_CARD_RADIUS);
    }
    response
}

/// Paints the light rim, top specular line and inner bottom shade of a glass
/// pane. Shared by cards and dialogs so both use the same material.
pub fn paint_glass_edge(painter: &egui::Painter, rect: egui::Rect, radius: u8) {
    let radius = CornerRadius::same(radius);
    let painter = painter.with_clip_rect(rect);
    painter.rect_stroke(
        rect.shrink(0.6),
        radius,
        Stroke::new(1.0_f32, alpha(Color32::WHITE, 205)),
        egui::StrokeKind::Inside,
    );

    let inset = radius.nw as f32;
    painter.line_segment(
        [
            egui::pos2(rect.left() + inset, rect.top() + 1.4),
            egui::pos2(rect.right() - inset, rect.top() + 1.4),
        ],
        Stroke::new(1.0_f32, alpha(Color32::WHITE, 240)),
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + inset, rect.bottom() - 1.3),
            egui::pos2(rect.right() - inset, rect.bottom() - 1.3),
        ],
        Stroke::new(1.0_f32, alpha(Color32::BLACK, 24)),
    );
}

/// Frame for modal dialogs. Glass gets a denser frosted pane than a card so
/// it stays readable over the dimmed backdrop.
pub fn dialog() -> egui::Frame {
    if palette().glass {
        return egui::Frame::new()
            .fill(alpha(Color32::WHITE, 226))
            .stroke(Stroke::new(1.0_f32, alpha(Color32::WHITE, 240)))
            .corner_radius(CornerRadius::same(GLASS_DIALOG_RADIUS))
            .inner_margin(Margin::same(24))
            .shadow(Shadow {
                offset: [0, 18],
                blur: 48,
                spread: 0,
                color: Color32::from_black_alpha(52),
            });
    }

    egui::Frame::new()
        .fill(panel())
        .stroke(Stroke::new(1.0_f32, line()))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(24))
        .shadow(shadow())
}

/// Adds the dialog rim after a modal has been shown, on the modal's own layer.
pub fn paint_dialog_edge(ctx: &egui::Context, layer: egui::LayerId, rect: egui::Rect) {
    if palette().glass {
        paint_glass_edge(&ctx.layer_painter(layer), rect, GLASS_DIALOG_RADIUS);
    }
}

/// Paints the simulated glass and seasonal wallpapers. Themes without a
/// wallpaper keep their flat fill, so this is a no-op for them.
pub fn paint_backdrop(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();

    match current_theme().wallpaper() {
        Wallpaper::Glass => glass_backdrop(painter, rect),
        Wallpaper::Halloween => halloween_backdrop(painter, rect),
        Wallpaper::Winter => winter_backdrop(painter, rect),
        Wallpaper::Neon => neon_backdrop(painter, rect),
        Wallpaper::Terminal => terminal_backdrop(painter, rect),
        Wallpaper::Spring => spring_backdrop(painter, rect),
        Wallpaper::Summer => summer_backdrop(painter, rect),
        Wallpaper::Autumn => autumn_backdrop(painter, rect),
        Wallpaper::Valentine => valentine_backdrop(painter, rect),
        Wallpaper::Galaxy => galaxy_backdrop(painter, rect),
        Wallpaper::Aurora => aurora_backdrop(painter, rect),
        Wallpaper::None => {}
    }
}

fn glass_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    // A saturated wallpaper rather than a pale wash: the translucent panes
    // need real colour behind them for the frosted effect to read at all.
    quad_gradient(
        painter,
        rect,
        Color32::from_rgb(166, 208, 255),
        Color32::from_rgb(216, 196, 255),
        Color32::from_rgb(255, 199, 224),
        Color32::from_rgb(166, 240, 224),
    );

    let extent = rect.width().max(rect.height());
    let blobs = [
        (0.04, 0.02, 0.66, Color32::from_rgb(64, 142, 255)),
        (0.96, 0.12, 0.56, Color32::from_rgb(160, 110, 255)),
        (0.88, 0.96, 0.54, Color32::from_rgb(255, 92, 158)),
        (0.02, 0.94, 0.62, Color32::from_rgb(42, 214, 190)),
        (0.50, 0.42, 0.44, Color32::from_rgb(255, 190, 120)),
        (0.64, -0.04, 0.38, Color32::from_rgb(92, 190, 255)),
    ];
    for (x, y, radius, color) in blobs {
        let center = egui::pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        );
        blob(painter, center, extent * radius, alpha(color, 54));
    }

    // A soft top light, like a window reflecting into the glass.
    vertical_gradient(
        painter,
        egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.right(), rect.top() + rect.height() * 0.46),
        ),
        alpha(Color32::WHITE, 42),
        Color32::TRANSPARENT,
    );

    // A faint diagonal sheen adds depth without introducing a visible edge.
    let sheen = egui::Rect::from_min_max(
        egui::pos2(rect.left() - rect.width() * 0.2, rect.top()),
        egui::pos2(
            rect.left() + rect.width() * 0.55,
            rect.top() + rect.height() * 0.7,
        ),
    );
    blob(
        painter,
        sheen.center(),
        extent * 0.5,
        alpha(Color32::WHITE, 18),
    );
}

fn halloween_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(26, 17, 34),
        Color32::from_rgb(10, 7, 15),
    );

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.14,
            rect.top() + rect.height() * 0.92,
        ),
        extent * 0.60,
        alpha(Color32::from_rgb(255, 112, 24), 44),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.92,
            rect.top() + rect.height() * 0.10,
        ),
        extent * 0.52,
        alpha(Color32::from_rgb(121, 74, 190), 52),
    );

    let bats = [
        (0.13, 0.12, 12.0),
        (0.31, 0.07, 8.0),
        (0.83, 0.24, 10.0),
        (0.93, 0.58, 7.0),
        (0.20, 0.71, 9.0),
    ];
    for (x, y, size) in bats {
        bat(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            alpha(Color32::from_rgb(255, 190, 120), 42),
        );
    }
}

fn winter_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(226, 240, 253),
        Color32::from_rgb(248, 252, 255),
    );

    let extent = rect.width().max(rect.height());
    for (x, y, radius, color) in [
        (0.12, 0.16, 0.52, Color32::from_rgb(126, 186, 255)),
        (0.86, 0.80, 0.48, Color32::from_rgb(168, 224, 255)),
        (0.92, 0.12, 0.34, Color32::from_rgb(255, 221, 150)),
    ] {
        blob(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            extent * radius,
            alpha(color, 34),
        );
    }

    for index in 0..72 {
        let x = ((index * 37 + 11) % 101) as f32 / 100.0;
        let y = ((index * 53 + 7) % 97) as f32 / 96.0;
        let radius = 0.7 + ((index * 7) % 5) as f32 * 0.34;
        painter.circle_filled(
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            radius,
            alpha(Color32::WHITE, 150),
        );
    }

    for (x, y, size) in [(0.80, 0.10, 9.0), (0.16, 0.84, 7.0), (0.58, 0.30, 5.0)] {
        snowflake(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            alpha(Color32::from_rgb(120, 178, 226), 66),
        );
    }
}

fn neon_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 0, Color32::from_rgb(7, 7, 15));

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.08,
            rect.top() + rect.height() * 0.88,
        ),
        extent * 0.64,
        alpha(Color32::from_rgb(0, 220, 255), 42),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.92,
            rect.top() + rect.height() * 0.12,
        ),
        extent * 0.56,
        alpha(Color32::from_rgb(255, 44, 190), 38),
    );

    let step = 46.0;
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            Stroke::new(1.0_f32, alpha(Color32::from_rgb(0, 220, 255), 11)),
        );
        x += step;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0_f32, alpha(Color32::from_rgb(255, 44, 190), 9)),
        );
        y += step;
    }
}

fn terminal_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 0, Color32::from_rgb(4, 9, 5));

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.10,
            rect.top() + rect.height() * 0.90,
        ),
        extent * 0.62,
        alpha(Color32::from_rgb(48, 255, 96), 26),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.90,
            rect.top() + rect.height() * 0.08,
        ),
        extent * 0.42,
        alpha(Color32::from_rgb(0, 150, 70), 22),
    );

    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0_f32, alpha(Color32::from_rgb(120, 255, 150), 9)),
        );
        y += 3.0;
    }
}

fn spring_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    quad_gradient(
        painter,
        rect,
        Color32::from_rgb(226, 242, 255),
        Color32::from_rgb(255, 228, 243),
        Color32::from_rgb(255, 238, 245),
        Color32::from_rgb(222, 248, 237),
    );

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.18,
            rect.top() + rect.height() * 0.13,
        ),
        extent * 0.26,
        alpha(Color32::from_rgb(255, 226, 152), 72),
    );
    // Soft hills made from very large blobs whose centers sit below the pane.
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.18,
            rect.bottom() + rect.height() * 0.34,
        ),
        extent * 0.64,
        alpha(Color32::from_rgb(128, 208, 166), 30),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.90,
            rect.bottom() + rect.height() * 0.38,
        ),
        extent * 0.56,
        alpha(Color32::from_rgb(255, 154, 196), 26),
    );

    for index in 0..30 {
        let x = ((index * 37 + 13) % 101) as f32 / 100.0;
        let y = ((index * 53 + 7) % 97) as f32 / 96.0;
        let size = 2.4 + ((index * 7) % 5) as f32 * 0.75;
        let angle = ((index * 29) % 360) as f32 * std::f32::consts::PI / 180.0;
        let color = if index % 3 == 0 {
            alpha(Color32::WHITE, 140)
        } else {
            alpha(Color32::from_rgb(238, 112, 164), 118)
        };
        petal(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            angle,
            color,
        );
    }

    for index in 0..16 {
        let x = ((index * 61 + 17) % 101) as f32 / 100.0;
        let y = ((index * 41 + 23) % 97) as f32 / 96.0;
        painter.circle_filled(
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            1.0 + ((index * 3) % 3) as f32 * 0.5,
            alpha(Color32::WHITE, 120),
        );
    }
}

fn summer_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    let horizon = rect.top() + rect.height() * 0.66;
    vertical_gradient(
        painter,
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), horizon)),
        Color32::from_rgb(170, 222, 247),
        Color32::from_rgb(235, 250, 255),
    );
    vertical_gradient(
        painter,
        egui::Rect::from_min_max(egui::pos2(rect.left(), horizon), rect.max),
        Color32::from_rgb(86, 196, 220),
        Color32::from_rgb(22, 126, 166),
    );

    let extent = rect.width().max(rect.height());
    let sun = egui::pos2(
        rect.left() + rect.width() * 0.72,
        rect.top() + rect.height() * 0.05,
    );
    blob(
        painter,
        sun,
        extent * 0.16,
        alpha(Color32::from_rgb(255, 224, 128), 96),
    );
    painter.circle_filled(sun, extent * 0.038, Color32::from_rgb(255, 239, 176));

    painter.line_segment(
        [
            egui::pos2(rect.left(), horizon),
            egui::pos2(rect.right(), horizon),
        ],
        Stroke::new(1.0_f32, alpha(Color32::WHITE, 78)),
    );

    for index in 0..7 {
        let y = horizon + rect.height() * (0.04 + index as f32 * 0.045);
        let x = rect.left() + rect.width() * (((index * 29 + 9) % 71) as f32 / 100.0);
        let length = rect.width() * (0.14 + ((index * 13) % 9) as f32 * 0.03);
        painter.line_segment(
            [egui::pos2(x, y), egui::pos2(x + length, y)],
            Stroke::new(1.2_f32, alpha(Color32::WHITE, 52)),
        );
    }

    for (x, y, size) in [(0.16, 0.14, 4.0), (0.27, 0.09, 3.0), (0.38, 0.20, 2.5)] {
        bird(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            alpha(Color32::from_rgb(32, 84, 102), 90),
        );
    }
}

fn autumn_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    quad_gradient(
        painter,
        rect,
        Color32::from_rgb(255, 236, 200),
        Color32::from_rgb(255, 214, 166),
        Color32::from_rgb(241, 196, 160),
        Color32::from_rgb(252, 225, 194),
    );

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.82,
            rect.top() + rect.height() * 0.76,
        ),
        extent * 0.30,
        alpha(Color32::from_rgb(255, 166, 72), 58),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.14,
            rect.bottom() + rect.height() * 0.30,
        ),
        extent * 0.58,
        alpha(Color32::from_rgb(150, 142, 74), 24),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.88,
            rect.bottom() + rect.height() * 0.32,
        ),
        extent * 0.54,
        alpha(Color32::from_rgb(170, 88, 44), 24),
    );

    let palette = [
        Color32::from_rgb(190, 82, 36),
        Color32::from_rgb(214, 124, 42),
        Color32::from_rgb(160, 68, 34),
        Color32::from_rgb(226, 158, 56),
    ];
    for index in 0..28 {
        let x = ((index * 43 + 11) % 101) as f32 / 100.0;
        let y = ((index * 59 + 3) % 97) as f32 / 96.0;
        let size = 2.8 + ((index * 11) % 6) as f32 * 0.85;
        let angle = ((index * 47) % 360) as f32 * std::f32::consts::PI / 180.0;
        let color = palette[index % palette.len()];
        leaf(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            angle,
            alpha(color, 132),
            alpha(Color32::from_rgb(110, 52, 24), 72),
        );
    }
}

fn valentine_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(255, 226, 236),
        Color32::from_rgb(255, 247, 250),
    );

    let extent = rect.width().max(rect.height());
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.12,
            rect.top() + rect.height() * 0.16,
        ),
        extent * 0.44,
        alpha(Color32::from_rgb(255, 150, 184), 48),
    );
    blob(
        painter,
        egui::pos2(
            rect.left() + rect.width() * 0.88,
            rect.top() + rect.height() * 0.84,
        ),
        extent * 0.42,
        alpha(Color32::from_rgb(230, 80, 128), 32),
    );

    for index in 0..14 {
        let x = ((index * 53 + 7) % 101) as f32 / 100.0;
        let y = ((index * 31 + 19) % 97) as f32 / 96.0;
        let radius = 6.0 + ((index * 9) % 15) as f32;
        painter.circle_filled(
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            radius,
            alpha(Color32::WHITE, 24),
        );
    }

    let palette = [
        Color32::from_rgb(218, 48, 90),
        Color32::from_rgb(240, 106, 138),
        Color32::from_rgb(255, 154, 178),
    ];
    for index in 0..16 {
        let x = ((index * 41 + 13) % 101) as f32 / 100.0;
        let y = ((index * 67 + 5) % 97) as f32 / 96.0;
        let size = 3.0 + ((index * 7) % 5) as f32 * 1.1;
        heart(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            alpha(palette[index % palette.len()], 130),
        );
    }
}

fn galaxy_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(4, 6, 22),
        Color32::from_rgb(13, 18, 46),
    );

    let extent = rect.width().max(rect.height());
    for (x, y, radius, color) in [
        (0.18, 0.30, 0.56, Color32::from_rgb(92, 64, 220)),
        (0.82, 0.72, 0.48, Color32::from_rgb(30, 120, 220)),
        (0.68, 0.16, 0.38, Color32::from_rgb(180, 60, 180)),
        (0.38, 0.58, 0.30, Color32::from_rgb(70, 170, 230)),
    ] {
        blob(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            extent * radius,
            alpha(color, 34),
        );
    }

    for index in 0..72 {
        let x = ((index * 37 + 5) % 101) as f32 / 100.0;
        let y = ((index * 61 + 23) % 97) as f32 / 96.0;
        let radius = 0.6 + ((index * 7) % 5) as f32 * 0.35;
        let color = alpha(Color32::WHITE, 130 + ((index * 17) % 90) as u8);
        painter.circle_filled(
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            radius,
            color,
        );
    }

    for (x, y, size) in [(0.30, 0.18, 4.5), (0.72, 0.44, 3.5), (0.14, 0.64, 3.0)] {
        sparkle(
            painter,
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            size,
            alpha(Color32::WHITE, 170),
        );
    }

    let planet = egui::pos2(
        rect.left() + rect.width() * 0.64,
        rect.top() + rect.height() * 0.055,
    );
    let planet_radius = extent * 0.028;
    let ring: Vec<egui::Pos2> = (0..=40)
        .map(|index| {
            let angle = index as f32 / 40.0 * std::f32::consts::TAU;
            planet
                + egui::vec2(
                    angle.cos() * planet_radius * 1.95,
                    angle.sin() * planet_radius * 0.55,
                )
        })
        .collect();
    painter.add(egui::Shape::line(
        ring,
        Stroke::new(1.3_f32, alpha(Color32::from_rgb(205, 196, 255), 150)),
    ));
    painter.circle_filled(
        planet,
        planet_radius,
        alpha(Color32::from_rgb(164, 154, 216), 230),
    );
    painter.circle_filled(
        planet + egui::vec2(planet_radius * 0.35, planet_radius * 0.25),
        planet_radius * 0.72,
        alpha(Color32::from_rgb(114, 104, 172), 210),
    );
}

fn aurora_backdrop(painter: &egui::Painter, rect: egui::Rect) {
    vertical_gradient(
        painter,
        rect,
        Color32::from_rgb(3, 9, 15),
        Color32::from_rgb(9, 22, 30),
    );

    for index in 0..64 {
        let x = ((index * 37 + 11) % 101) as f32 / 100.0;
        let y = ((index * 47 + 7) % 91) as f32 / 90.0;
        painter.circle_filled(
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            0.6 + ((index * 5) % 4) as f32 * 0.35,
            alpha(Color32::WHITE, 90 + ((index * 19) % 110) as u8),
        );
    }

    aurora_ribbon(
        painter,
        rect,
        0.20,
        0.055,
        0.4,
        28.0,
        Color32::from_rgb(70, 226, 164),
    );
    aurora_ribbon(
        painter,
        rect,
        0.33,
        0.050,
        2.1,
        20.0,
        Color32::from_rgb(58, 190, 220),
    );
    aurora_ribbon(
        painter,
        rect,
        0.45,
        0.042,
        4.2,
        16.0,
        Color32::from_rgb(148, 122, 230),
    );

    // A dark horizon anchors the sky and keeps the bottom text readable.
    vertical_gradient(
        painter,
        egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - rect.height() * 0.22),
            rect.max,
        ),
        Color32::TRANSPARENT,
        alpha(Color32::from_rgb(4, 10, 14), 190),
    );
}

fn petal(painter: &egui::Painter, center: egui::Pos2, size: f32, angle: f32, color: Color32) {
    let direction = egui::vec2(angle.cos(), angle.sin());
    let side = egui::vec2(-direction.y, direction.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + direction * size,
            center + side * size * 0.48 - direction * size * 0.15,
            center - direction * size * 0.85,
            center - side * size * 0.48 - direction * size * 0.15,
        ],
        color,
        Stroke::NONE,
    ));
}

fn leaf(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    angle: f32,
    fill: Color32,
    vein: Color32,
) {
    petal(painter, center, size, angle, fill);
    let direction = egui::vec2(angle.cos(), angle.sin());
    painter.line_segment(
        [
            center - direction * size * 0.45,
            center + direction * size * 0.62,
        ],
        Stroke::new(0.8_f32, vein),
    );
}

fn heart(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    let lobe = size * 0.34;
    painter.circle_filled(center + egui::vec2(-size * 0.30, -size * 0.16), lobe, color);
    painter.circle_filled(center + egui::vec2(size * 0.30, -size * 0.16), lobe, color);
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-size * 0.64, -size * 0.12),
            center + egui::vec2(size * 0.64, -size * 0.12),
            center + egui::vec2(0.0, size * 0.78),
        ],
        color,
        Stroke::NONE,
    ));
}

fn sparkle(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    painter.circle_filled(center, size * 0.32, color);
    let stroke = Stroke::new(0.8_f32, color);
    painter.line_segment(
        [
            center - egui::vec2(size, 0.0),
            center + egui::vec2(size, 0.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center - egui::vec2(0.0, size),
            center + egui::vec2(0.0, size),
        ],
        stroke,
    );
}

fn bird(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    painter.line_segment(
        [
            center + egui::vec2(-size, 0.0),
            center + egui::vec2(0.0, -size * 0.35),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(0.0, -size * 0.35),
            center + egui::vec2(size, 0.0),
        ],
        stroke,
    );
}

fn aurora_ribbon(
    painter: &egui::Painter,
    rect: egui::Rect,
    base_y: f32,
    amplitude: f32,
    phase: f32,
    width: f32,
    color: Color32,
) {
    let points: Vec<egui::Pos2> = (0..=40)
        .map(|index| {
            let t = index as f32 / 40.0;
            let y = base_y
                + amplitude * (t * std::f32::consts::TAU * 1.7 + phase).sin()
                + amplitude * 0.35 * (t * std::f32::consts::TAU * 3.3 + phase * 1.7).sin();
            egui::pos2(
                rect.left() + rect.width() * t,
                rect.top() + rect.height() * y,
            )
        })
        .collect();

    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(width, alpha(color, 12)),
    ));
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(width * 0.55, alpha(color, 20)),
    ));
    painter.add(egui::Shape::line(
        points,
        Stroke::new(width * 0.20, alpha(color, 30)),
    ));
}

fn quad_gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    top_left: Color32,
    top_right: Color32,
    bottom_right: Color32,
    bottom_left: Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top_left);
    mesh.colored_vertex(rect.right_top(), top_right);
    mesh.colored_vertex(rect.right_bottom(), bottom_right);
    mesh.colored_vertex(rect.left_bottom(), bottom_left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn vertical_gradient(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    quad_gradient(painter, rect, top, top, bottom, bottom);
}

fn bat(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    for direction in [-1.0_f32, 1.0] {
        painter.add(egui::Shape::convex_polygon(
            vec![
                center,
                center + egui::vec2(direction * size, -size * 0.52),
                center + egui::vec2(direction * size * 0.94, size * 0.10),
            ],
            color,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(center, size * 0.19, color);
}

fn snowflake(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new(1.0_f32, color);
    for index in 0..3 {
        let angle = index as f32 / 3.0 * std::f32::consts::PI;
        let direction = egui::vec2(angle.cos(), angle.sin()) * size;
        painter.line_segment([center - direction, center + direction], stroke);
    }
}

fn blob(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    const SEGMENTS: usize = 32;
    let mut mesh = egui::Mesh::default();
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: egui::epaint::WHITE_UV,
        color,
    });
    for index in 0..=SEGMENTS {
        let angle = index as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + egui::vec2(angle.cos(), angle.sin()) * radius,
            uv: egui::epaint::WHITE_UV,
            color: Color32::TRANSPARENT,
        });
    }
    for index in 1..=SEGMENTS {
        mesh.add_triangle(0, index as u32, index as u32 + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A tinted strip for the current status notice.
pub fn notice_frame(tone: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(alpha(tone, if palette().dark { 18 } else { 12 }))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(9, 3))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let files: [(&str, &'static [u8]); 5] = [
        (
            SANS_REGULAR,
            include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf").as_slice(),
        ),
        (
            SANS_MEDIUM,
            include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf").as_slice(),
        ),
        (
            SANS_SEMIBOLD,
            include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf").as_slice(),
        ),
        (
            MONO_REGULAR,
            include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf").as_slice(),
        ),
        (
            MONO_MEDIUM,
            include_bytes!("../../assets/fonts/IBMPlexMono-Medium.ttf").as_slice(),
        ),
    ];

    for (name, bytes) in files {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    prepend(&mut fonts, FontFamily::Proportional, SANS_REGULAR);
    prepend(&mut fonts, FontFamily::Monospace, MONO_REGULAR);

    fonts.families.insert(
        named_family(FAMILY_SANS_MEDIUM),
        vec![SANS_MEDIUM.to_owned(), SANS_REGULAR.to_owned()],
    );
    fonts.families.insert(
        named_family(FAMILY_SANS_SEMIBOLD),
        vec![SANS_SEMIBOLD.to_owned(), SANS_REGULAR.to_owned()],
    );
    fonts.families.insert(
        named_family(FAMILY_MONO_MEDIUM),
        vec![MONO_MEDIUM.to_owned(), MONO_REGULAR.to_owned()],
    );

    ctx.set_fonts(fonts);
}

fn prepend(fonts: &mut FontDefinitions, family: FontFamily, name: &str) {
    let entries = fonts.families.entry(family).or_default();
    entries.retain(|entry| entry != name);
    entries.insert(0, name.to_owned());
}

fn install_style(ctx: &egui::Context) {
    let base = ctx.style_of(egui::Theme::Light);
    let style = build_style((*base).clone(), palette());

    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn build_style(mut style: egui::Style, palette: Palette) -> egui::Style {
    style.animation_time = 0.16;
    style.spacing.item_spacing = Vec2::new(8.0, 3.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.interact_size = Vec2::new(44.0, 30.0);
    style.spacing.slider_width = 180.0;
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.window_margin = Margin::same(16);
    style.spacing.indent = 16.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();

    style.text_styles.insert(TextStyle::Small, sans(11.5));
    style.text_styles.insert(TextStyle::Body, sans(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, sans_medium(13.5));
    style.text_styles.insert(TextStyle::Monospace, mono(12.5));
    style
        .text_styles
        .insert(TextStyle::Heading, sans_semibold(22.0));

    style.visuals = visuals(palette);
    style
}

fn visuals(palette: Palette) -> egui::Visuals {
    let Palette {
        dark,
        bg,
        panel,
        input,
        line,
        text,
        warning,
        danger,
        accent,
        ..
    } = palette;

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = dark;
    visuals.panel_fill = bg;
    visuals.window_fill = if palette.glass {
        alpha(Color32::WHITE, 232)
    } else {
        panel
    };
    visuals.faint_bg_color = input;
    visuals.extreme_bg_color = if palette.glass {
        Color32::TRANSPARENT
    } else {
        panel
    };
    visuals.code_bg_color = input;
    visuals.window_stroke = Stroke::new(
        1.0_f32,
        if palette.glass {
            alpha(Color32::WHITE, 235)
        } else {
            line
        },
    );
    visuals.window_corner_radius =
        CornerRadius::same(if palette.glass { GLASS_CARD_RADIUS } else { 16 });
    visuals.window_shadow = shadow_for(dark);
    visuals.popup_shadow = shadow_for(dark);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.hyperlink_color = accent;
    visuals.warn_fg_color = warning;
    visuals.error_fg_color = danger;
    visuals.selection.bg_fill = alpha(accent, if dark { 64 } else { 40 });
    visuals.selection.stroke = Stroke::new(1.0_f32, accent);
    visuals.slider_trailing_fill = true;
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, accent);

    let widget_radius = if palette.glass { 10 } else { 7 };
    let idle_fill = if palette.glass {
        alpha(Color32::WHITE, 155)
    } else {
        input
    };
    let idle_stroke = if palette.glass {
        Stroke::new(1.0_f32, alpha(Color32::BLACK, 24))
    } else {
        Stroke::new(1.0_f32, line)
    };

    let mut widgets = visuals.widgets.clone();
    widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: if palette.glass {
            alpha(Color32::WHITE, 72)
        } else {
            bg
        },
        weak_bg_fill: if palette.glass {
            alpha(Color32::WHITE, 72)
        } else {
            bg
        },
        bg_stroke: Stroke::NONE,
        corner_radius: CornerRadius::same(8),
        fg_stroke: Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: idle_fill,
        weak_bg_fill: idle_fill,
        bg_stroke: idle_stroke,
        corner_radius: CornerRadius::same(widget_radius),
        fg_stroke: Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: if palette.glass {
            alpha(mix(Color32::WHITE, accent, 0.12), 205)
        } else {
            mix(input, accent, if dark { 0.14 } else { 0.08 })
        },
        weak_bg_fill: if palette.glass {
            alpha(mix(Color32::WHITE, accent, 0.12), 205)
        } else {
            mix(input, accent, if dark { 0.14 } else { 0.08 })
        },
        bg_stroke: Stroke::new(1.0_f32, alpha(accent, 170)),
        corner_radius: CornerRadius::same(widget_radius),
        fg_stroke: Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    widgets.active = egui::style::WidgetVisuals {
        bg_fill: alpha(
            accent,
            if palette.glass {
                44
            } else if dark {
                52
            } else {
                26
            },
        ),
        weak_bg_fill: alpha(
            accent,
            if palette.glass {
                44
            } else if dark {
                52
            } else {
                26
            },
        ),
        bg_stroke: Stroke::new(1.0_f32, accent),
        corner_radius: CornerRadius::same(widget_radius),
        fg_stroke: Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    widgets.open = egui::style::WidgetVisuals {
        bg_fill: if palette.glass {
            alpha(mix(Color32::WHITE, accent, 0.16), 215)
        } else {
            mix(input, accent, if dark { 0.18 } else { 0.10 })
        },
        weak_bg_fill: if palette.glass {
            alpha(mix(Color32::WHITE, accent, 0.16), 215)
        } else {
            mix(input, accent, if dark { 0.18 } else { 0.10 })
        },
        bg_stroke: Stroke::new(1.0_f32, accent),
        corner_radius: CornerRadius::same(widget_radius),
        fg_stroke: Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    visuals.widgets = widgets;

    visuals
}

fn shadow_for(dark: bool) -> Shadow {
    Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 120 } else { 38 }),
    }
}

fn named_family(name: &str) -> FontFamily {
    FontFamily::Name(Arc::from(name))
}
