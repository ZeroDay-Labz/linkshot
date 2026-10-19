//! Enumerations with the spellings used in `.snagx` files and settings.

use serde::{Deserialize, Serialize};

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $( $(#[$vm:meta])* $variant:ident $(= $alias:literal)? ),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $( $(#[$vm])* $(#[serde(alias = $alias)])? $variant, )+ }
        impl $name {
            pub const ALL: &'static [$name] = &[ $( $name::$variant, )+ ];
            pub fn as_str(self) -> &'static str { match self { $( $name::$variant => stringify!($variant), )+ } }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.as_str()) }
        }
        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| format!("unknown {} {s:?}", stringify!($name)))
            }
        }
    };
}

str_enum!(#[derive(Default)] CaptureMode { #[default] CaptureModeAllInOne, CaptureModeImage, CaptureModeVideo });

str_enum!(#[derive(Default)] SelectionMode {
    #[default] SelectionModeRegion, SelectionModeWindow, SelectionModeScrollingWindow, SelectionModeMenu,
    SelectionModeFreehand, SelectionModeFullscreen, SelectionModePanoramic, SelectionModeMultipleArea,
    SelectionModeClipboard, SelectionModeFile, SelectionModeObject, SelectionModeOcr, SelectionModeStepCapture,
});

impl SelectionMode {
    /// Text shown in the Capture window dropdown.
    pub fn label(self) -> &'static str {
        match self {
            Self::SelectionModeRegion => "Region",
            Self::SelectionModeWindow => "Window",
            Self::SelectionModeScrollingWindow => "Automatic Scrolling",
            Self::SelectionModeMenu => "Menu",
            Self::SelectionModeFreehand => "Freehand",
            Self::SelectionModeFullscreen => "Fullscreen",
            Self::SelectionModePanoramic => "Scrolling",
            Self::SelectionModeMultipleArea => "Multiple Area",
            Self::SelectionModeClipboard => "Clipboard",
            Self::SelectionModeFile => "File",
            Self::SelectionModeObject => "Object",
            Self::SelectionModeOcr => "Grab Text",
            Self::SelectionModeStepCapture => "Steps",
        }
    }
    /// Items of the Image tab Selection menu, in menu order (Advanced sub-menu excluded).
    pub const IMAGE_MENU: &'static [SelectionMode] = &[
        Self::SelectionModeRegion, Self::SelectionModeWindow, Self::SelectionModeFullscreen,
        Self::SelectionModePanoramic, Self::SelectionModeScrollingWindow, Self::SelectionModeOcr,
    ];
    pub const IMAGE_MENU_ADVANCED: &'static [SelectionMode] = &[
        Self::SelectionModeObject, Self::SelectionModeMultipleArea, Self::SelectionModeMenu,
        Self::SelectionModeClipboard, Self::SelectionModeFreehand, Self::SelectionModeFile,
    ];
    pub const ALL_IN_ONE_MENU: &'static [SelectionMode] =
        &[Self::SelectionModeRegion, Self::SelectionModeWindow, Self::SelectionModeFullscreen];
    pub const VIDEO_MENU: &'static [SelectionMode] = &[Self::SelectionModeRegion, Self::SelectionModeWindow];
}

str_enum!(#[derive(Default)] RegionSelectionType { #[default] RegionSelectionPrompt, RegionSelectionFixed });
str_enum!(#[derive(Default)] WindowSelectionType { #[default] WindowSelectionActive, WindowSelectionPrompt });
str_enum!(#[derive(Default)] WindowSizeSelectionType { #[default] WindowSizeSelectionActual, WindowSizeSelectionFixed });
str_enum!(#[derive(Default)] TimedCaptureType { #[default] Delay, Interval, Schedule });
str_enum!(#[derive(Default)] IntervalUnits { #[default] Seconds, Minutes, Hours });
str_enum!(#[derive(Default)] WebcamPlacement { Fullscreen, TopLeft, TopRight, BottomLeft, #[default] BottomRight });
str_enum!(#[derive(Default)] PreviewClipShape { #[default] Circle, Rectangle, RoundedRectangle, Square, RoundedSquare });

str_enum!(#[derive(Default)] EffectType {
    #[default] EffectNone, EffectFilter, EffectBorder, EffectShadow, EffectCaption, EffectColorReplacement, EffectScale,
    EffectEdge, EffectTrim, EffectColorAdjustment, EffectRotation, EffectWatermark, EffectPerspectiveShear,
    EffectPageCurl, EffectSpotlightMagnify, EffectImageResolution,
});
impl EffectType {
    pub fn label(self) -> &'static str {
        match self {
            Self::EffectNone => "None",
            Self::EffectFilter => "Filters",
            Self::EffectBorder => "Border",
            Self::EffectShadow => "Shadow",
            Self::EffectCaption => "Capture Info",
            Self::EffectColorReplacement => "Color Replacement",
            Self::EffectScale => "Scale",
            Self::EffectEdge => "Edges",
            Self::EffectTrim => "Trim",
            Self::EffectColorAdjustment => "Color Adjustment",
            Self::EffectRotation => "Rotation",
            Self::EffectWatermark => "Watermark",
            Self::EffectPerspectiveShear => "Perspective",
            Self::EffectPageCurl => "Page Curl",
            Self::EffectSpotlightMagnify => "Spotlight & Magnify",
            Self::EffectImageResolution => "Image Resolution",
        }
    }
}
str_enum!(#[derive(Default)] FilterType { #[default] Soften, Sharpen, Opacity, Monochrome, Grayscale, Invert });
str_enum!(#[derive(Default)] EdgeType { #[default] Torn, Wave, Saw, Sharktooth, Fade, Bevel, NoEdge });
str_enum!(#[derive(Default)] PerspectiveType { #[default] Perspective, Shear });
str_enum!(#[derive(Default)] ScaleType { #[default] PercentageScale, ScaleToWidth, ScaleToHeight, ScaleToWidthHeight });
str_enum!(#[derive(Default)] CaptionOutsidePosition { Left, Right, Top, #[default] Bottom });
str_enum!(#[derive(Default)] ColorReplacementMode { #[default] Replace, Swap });
str_enum!(#[derive(Default)] TimeStampOrder { #[default] TimeThenDate, TimeOnly, DateThenTime, DateOnly });

// Editor tools. `VectorSelection` is the **Move** tool, `SimplifiedUI` is **Simplify**.
str_enum!(#[derive(Default)] ToolMode {
    None, Selection, Callouts, Pen, Highlight, #[default] Arrow, Shape, Line, Fill, Eraser, Step, Stamp, Blur, Crop,
    VectorSelection, CutOut, Text, Magnify, MagicWand, SimplifiedUI, Favorites, DropZone, LastTool,
});
impl ToolMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Selection => "Selection",
            Self::Callouts => "Callout",
            Self::Pen => "Pen",
            Self::Highlight => "Highlighter",
            Self::Arrow => "Arrow",
            Self::Shape => "Shape",
            Self::Line => "Line",
            Self::Fill => "Fill",
            Self::Eraser => "Eraser",
            Self::Step => "Step",
            Self::Stamp => "Stamp",
            Self::Blur => "Blur",
            Self::Crop => "Crop",
            Self::VectorSelection => "Move",
            Self::CutOut => "Cut Out",
            Self::Text => "Text",
            Self::Magnify => "Magnify",
            Self::MagicWand => "Magic Wand",
            Self::SimplifiedUI => "Simplify",
            Self::Favorites => "Favorites",
            Self::DropZone => "Drop Zone",
            Self::LastTool => "",
        }
    }
    /// Tools that may appear on the toolbar / in "More", in toolbar order.
    pub const TOOLBAR_TOOLS: &'static [ToolMode] = &[
        Self::Favorites, Self::Arrow, Self::Text, Self::Callouts, Self::Shape, Self::Stamp, Self::Fill,
        Self::VectorSelection, Self::Selection, Self::Blur, Self::SimplifiedUI, Self::MagicWand, Self::Crop,
        Self::CutOut, Self::Pen, Self::Line, Self::Highlight, Self::Step, Self::Eraser, Self::Magnify,
    ];
    /// Tools whose quick styles come from a theme.
    pub fn supports_themes(self) -> bool {
        matches!(self, Self::Arrow | Self::Text | Self::Callouts | Self::Shape | Self::Line | Self::Pen
            | Self::Highlight | Self::Step | Self::Fill | Self::Magnify | Self::Favorites)
    }
}

str_enum!(#[derive(Default)] ToolBarItemType { #[default] Tool, Effect, Output, Command, Spacer, EditMore });
str_enum!(#[derive(Default)] ToolBarCommandType {
    #[default] None, Undo, Redo, NewImage, NewFromClipboard, Open, Save, SaveAs, Duplicate, DeleteObject, DeleteImage,
    FlattenAll, RotateImage, AutoTrim, PageSetup, Print, Ocr,
});
str_enum!(#[derive(Default)] TrayItemType { #[default] Image, Video, AnimatedGif });

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TrayThumbnailSize { Large = 0, #[default] Medium = 1, Small = 2 }
impl TrayThumbnailSize {
    pub fn pixels(self) -> (f32, f32) {
        match self { Self::Small => (80.0, 45.0), Self::Medium => (160.0, 90.0), Self::Large => (320.0, 180.0) }
    }
}

// Arrow/line end styles as written in `.snagx` (`ArrowStart`/`ArrowEnd`).
str_enum!(#[derive(Default)] ArrowLineEndType {
    #[default] Flat, Round, RoundAnchor, SquareAnchor, DiamondAnchor, TArrow, EquilateralArrow, BarbedArrow, LineArrow,
    RoundedArrow, TaperArrow,
});
str_enum!(#[derive(Default)] ArrowStyle { #[default] ASFilledBarbed, ASFilledEquilateral, ASLine, ASFilledRounded });
str_enum!(#[derive(Default)] LineCap { #[default] Flat, Round, RoundAnchor, SquareAnchor, DiamondAnchor, TArrow, ArrowAnchor });
str_enum!(#[derive(Default)] LineType { #[default] Single, Bezier, PolyBezier });
str_enum!(#[derive(Default)] DashType { #[default] Solid, Dash, Dot, DashDot, DashDotDot });

// `.snagx` spells these `Gaussian` / `Pixellate`; XML uses `BlurTypeSmooth` / `BlurTypePixelate`.
str_enum!(#[derive(Default)] BlurType { #[default] Gaussian = "BlurTypeSmooth", Pixellate = "BlurTypePixelate" });

str_enum!(#[derive(Default)] CalloutShape {
    CTArrow1, CTArrow2, CTArrow3, CTArrow4, CTArrow5, CTArrow6,
    CTBalloon1, CTBalloon2, CTBalloon3, CTBalloon4, CTBalloon5, CTBalloon6, CTBalloon7, CTBalloon8, CTBalloon9,
    CTBalloon10, CTBalloon11, CTBalloon12,
    CTBasicSpeechBubble1, CTBasicSpeechBubble2, CTBasicThoughtBubble1, CTBasicThoughtBubble2,
    CTBasicArrowText1, CTBasicArrowText2, #[default] CTRoundedRectWithArrow,
});
str_enum!(#[derive(Default)] CalloutTailStyle { None, Triangle, ThoughtBubble, OvalBubble, Remix, CircleBubble, #[default] Arrow });
str_enum!(#[derive(Default)] CutOutType {
    #[default] HorzNoneCutOut, HorzFadeCutOut, HorzTearCutOut, HorzSharktoothCutOut, HorzWaveCutOut, HorzSawCutOut,
    VertNoneCutOut, VertFadeCutOut, VertTearCutOut, VertSharktoothCutOut, VertWaveCutOut, VertSawCutOut, CutOutMax,
});
str_enum!(#[derive(Default)] PenShape { #[default] PenSquare, PenRound, PenHorizontal, PenVertical, PenRightSlash, PenLeftSlash });
str_enum!(#[derive(Default)] SelectionShape { #[default] SSRectangle, SSEllipse, SSFreehand, SSPolygon, SSMagicPoint, SSMagicLasso, SSMultiple });
str_enum!(#[derive(Default)] ShapeShape { #[default] Rectangle, RoundedRectangle, Ellipse, Polygon });
str_enum!(#[derive(Default)] SimplifyShape { #[default] Rectangle, RoundedRectangle, Ellipse, TextLeft, TextCenter, TextRight });
str_enum!(#[derive(Default)] SimplifyPaletteType { #[default] ImageColors, Custom, UserGenerated });

// `.snagx` writes `Number`; XML writes `STNumeric`.
str_enum!(#[derive(Default)] StepSequenceType {
    #[default] Number = "STNumeric", UppercaseLetter = "STUppercaseLetter", LowercaseLetter = "STLowercaseLetter",
});
impl StepSequenceType {
    /// Label for the n-th step (1-based) in this sequence.
    pub fn label(self, n: u32) -> String {
        match self {
            Self::Number => n.to_string(),
            Self::UppercaseLetter | Self::LowercaseLetter => {
                // 1 → A, 26 → Z, 27 → AA …
                let mut n = n;
                let mut s = Vec::new();
                while n > 0 {
                    let r = ((n - 1) % 26) as u8;
                    s.push(b'A' + r);
                    n = (n - 1) / 26;
                }
                s.reverse();
                let s = String::from_utf8(s).unwrap_or_default();
                if self == Self::LowercaseLetter { s.to_lowercase() } else { s }
            }
        }
    }
}

// `.snagx` writes `Circle`; XML writes `NSCircle`.
str_enum!(#[derive(Default)] StepStyle {
    #[default] Circle = "NSCircle", Square = "NSSquare", CirclePointed = "NSCirclePointed", SquarePointed = "NSSquarePointed",
});
str_enum!(#[derive(Default)] HAlign { Left = "HAlignLeft", #[default] Center = "HAlignCenter", Right = "HAlignRight" });
str_enum!(#[derive(Default)] VAlign { Top = "VAlignTop", #[default] Center = "VAlignCenter", Bottom = "VAlignBottom" });
str_enum!(#[derive(Default)] SelectionFillOptions { #[default] BGFTransparent, BGFAutoFill });
str_enum!(#[derive(Default)] SelectionFillPresets { #[default] Speed, PhotoQuality });
str_enum!(#[derive(Default)] BorderStyle { Inset, #[default] Middle, Outset });
str_enum!(#[derive(Default)] SectionType { SectionTypeHeader, #[default] SectionTypeBody, SectionTypeFooter });
str_enum!(#[derive(Default)] GradientStyle { #[default] GradientStyleSolid, GradientStyleLinear, GradientStyleRadial });
str_enum!(#[derive(Default)] TextDeselectBehavior { #[default] Delete, Keep });

/// Built-in share destination identifiers (plugin ids).
pub mod output_ids {
    pub const NONE: &str = "0";
    pub const EMAIL: &str = "25231";
    pub const FTP: &str = "25232";
    pub const PROGRAM: &str = "25233";
    pub const CLIPBOARD: &str = "25234";
    pub const FILE: &str = "25235";
    pub const PRINT: &str = "25239";
    pub const WORD: &str = "{0BBFE402-CCA1-4f64-9322-13B66D841049}";
    pub const POWERPOINT: &str = "{25D005BF-FE63-4cce-AA25-CE952B1D9381}";
    pub const EXCEL: &str = "{54050FBB-F2AE-404b-8BFD-7EE3EC784A52}";
    pub const CAMTASIA: &str = "{293600C7-E7B6-4f06-9329-D8522A33C7E8}";
    pub const SCREENCAST: &str = "{672CDBDB-0270-4eb9-83EC-216377522D21}";
    pub const YOUTUBE: &str = "{841BFDCA-6A9A-4EBC-BC7E-194AA5DCE428}";
    pub const GOOGLE_DRIVE: &str = "{94330D48-EB33-49BB-87F1-AD8C0352C010}";
    pub const DROPBOX: &str = "{F7CA46A9-ACA5-45A6-967E-03FF5A282D01}";
    pub const SLACK: &str = "{BFDBD8EA-0B06-4D4E-8A1B-86E4082D5166}";
    pub const BOX: &str = "{7A1E68F2-E5EA-41EC-B6C8-2507449513C4}";
    pub const ONEDRIVE: &str = "{808B71EE-F742-4561-9EEA-B75D85DFF706}";
    pub const ONENOTE: &str = "{7E230376-4C82-4539-A828-CE1BB88A8F9F}";
    pub const EVERNOTE: &str = "{96A3DC28-0C3E-4CB3-95C7-F751ABC27F5D}";
    pub const TWITTER: &str = "{2485098E-1ACB-4f7c-9D6D-F207A433AC60}";
    pub const SHAREPOINT: &str = "{E2C1FC77-AE27-4E50-A61E-636C478AF6DA}";
    pub const TEAMS: &str = "{5147F54B-C590-4666-9EC4-DBAF293C9061}";

    pub fn title(id: &str) -> &'static str {
        match id {
            NONE => "None", EMAIL => "Email", FTP => "FTP", PROGRAM => "Program", CLIPBOARD => "Clipboard",
            FILE => "File", PRINT => "Printer", WORD => "Word", POWERPOINT => "PowerPoint", EXCEL => "Excel",
            CAMTASIA => "Camtasia", SCREENCAST => "Screencast", YOUTUBE => "YouTube", GOOGLE_DRIVE => "Google Drive",
            DROPBOX => "Dropbox", SLACK => "Slack", BOX => "Box", ONEDRIVE => "OneDrive", ONENOTE => "OneNote",
            EVERNOTE => "Evernote", TWITTER => "X", SHAREPOINT => "SharePoint", TEAMS => "Microsoft Teams",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_spelling() {
        assert_eq!(serde_json::to_string(&BlurType::Gaussian).unwrap(), "\"Gaussian\"");
        assert_eq!(serde_json::from_str::<BlurType>("\"BlurTypeSmooth\"").unwrap(), BlurType::Gaussian);
        assert_eq!(serde_json::from_str::<StepStyle>("\"NSCircle\"").unwrap(), StepStyle::Circle);
        assert_eq!("SelectionModeOcr".parse::<SelectionMode>().unwrap().label(), "Grab Text");
        assert_eq!(StepSequenceType::UppercaseLetter.label(27), "AA");
        assert_eq!(StepSequenceType::LowercaseLetter.label(2), "b");
    }
}
