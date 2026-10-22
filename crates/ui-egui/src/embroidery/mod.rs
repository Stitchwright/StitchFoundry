//! Embroidery mode UI: the stitch overlay on the canvas, the Embroidery and Stitch
//! Sequence panels, the stitch player and the digitizing widget.

pub mod account;
pub mod ask;
pub mod kiosk;
pub mod overlay;
pub mod panel;
pub mod profile_curve;
pub mod prompt;
pub mod publish;
pub mod sequence;
pub mod sewout;
pub mod stitch_list;
pub mod family;
pub mod stitchshop;
pub mod styles;
pub mod viewer;
pub mod widget;

use egui::Color32;

/// How stitches are shown on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StitchView {
    Off,
    /// Thin lines in thread colour.
    Lines,
    /// Shaded thread ("TrueView").
    #[default]
    TrueView,
}

/// How the artwork under the stitches is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArtView {
    Normal,
    /// Faded behind a fabric tint.
    #[default]
    Dimmed,
    Hidden,
}

/// The stitch player (slow redraw).
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub active: bool,
    pub playing: bool,
    /// Pattern records drawn so far.
    pub pos: f64,
    /// Records per second.
    pub speed: f64,
    /// What the travel buttons step by.
    pub travel: TravelBy,
}

impl Default for Player {
    fn default() -> Self {
        Player { active: false, playing: false, pos: 0.0, speed: 400.0, travel: TravelBy::default() }
    }
}

/// A machine function's name in the interface language: "Trim" is a thread cut here,
/// not the Pathfinder command of the same English name.
pub(crate) fn machine_word(s: &'static str) -> &'static str {
    crate::i18n::tr_ctx(crate::i18n::current(), "machine", s)
}

/// What the stitch player's travel buttons step by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TravelBy {
    Stitch,
    #[default]
    Object,
    Color,
    Trim,
    Function,
}

impl TravelBy {
    pub const ALL: [TravelBy; 5] = [TravelBy::Stitch, TravelBy::Object, TravelBy::Color, TravelBy::Trim, TravelBy::Function];

    pub fn label(self) -> &'static str {
        match self {
            TravelBy::Stitch => "Stitch",
            TravelBy::Object => "Object",
            TravelBy::Color => "Color",
            TravelBy::Trim => "Trim",
            TravelBy::Function => "Function",
        }
    }
}

/// Where the player lands one `by` step on from `pos` (records drawn so far), forwards
/// or back: one stitch, the end of an object, just after a colour change, a trim, or any
/// machine function (jump, trim, stop, colour change).
pub fn travel(plan: &vectorcraft_embroidery::StitchPlan, pos: usize, by: TravelBy, forward: bool) -> usize {
    use vectorcraft_embroidery::sf_core::Cmd;
    let st = &plan.pattern().stitches;
    let n = st.len();
    let after = |f: &dyn Fn(Cmd) -> bool| -> Vec<usize> { st.iter().enumerate().filter(|(_, s)| f(s.cmd)).map(|(i, _)| i + 1).collect() };
    let mut stops: Vec<usize> = match by {
        TravelBy::Stitch => return if forward { (pos + 1).min(n) } else { pos.saturating_sub(1) },
        TravelBy::Object => plan.built.spans.iter().map(|s| s.range.end).collect(),
        TravelBy::Color => after(&|c| c == Cmd::ColorChange),
        TravelBy::Trim => after(&|c| c == Cmd::Trim),
        TravelBy::Function => after(&|c| c != Cmd::Stitch),
    };
    stops.extend([0, n]);
    stops.sort_unstable();
    stops.dedup();
    if forward { stops.into_iter().find(|&k| k > pos).unwrap_or(n) } else { stops.into_iter().rev().find(|&k| k < pos).unwrap_or(0) }
}

/// The document object whose stitches the player drew last at `pos`.
pub fn object_at(plan: &vectorcraft_embroidery::StitchPlan, pos: usize) -> Option<vectorcraft_doc::NodeId> {
    let last = pos.checked_sub(1)?;
    let span = plan.built.spans.iter().rev().find(|s| s.range.start <= last)?;
    plan.nodes.get(span.index).copied()
}

pub struct EmbroideryUi {
    pub stitches: StitchView,
    pub art: ArtView,
    pub connectors: bool,
    pub needle_points: bool,
    /// Colour the canvas by how deep the thread lies (View › Density Map).
    pub density_map: bool,
    /// The density map's layers, for the plan it was measured on.
    pub density: Option<(usize, std::sync::Arc<vectorcraft_embroidery::sf_core::density::LayerGrid>)>,
    /// Show the selected objects' start and stop beads.
    pub beads: bool,
    /// Show the selected objects' inclination lines with any tool.
    pub guides: bool,
    /// Fabric colour under the stitches.
    pub fabric: Color32,
    pub player: Player,
    pub mesh: overlay::MeshCache,
    /// The digitizing widget.
    pub widget: widget::WidgetState,
    /// Auto-Digitize: thread colours to trace with, and whether the background is dropped.
    pub digitize_colors: u32,
    pub digitize_background: bool,
    /// The colour preview of the selected image (Auto-Digitize's colours, before digitizing).
    pub colour_preview: Option<ColourPreview>,
    /// Remove Background's sensitivity (0–100, 50 even), its before-and-after preview of the
    /// selected image, and where the preview's split line is (0 all after, 1 all before).
    pub cutout_sensitivity: u32,
    pub cutout_preview: Option<CutoutPreview>,
    pub cutout_split: f32,
    /// Photo Stitch's mode: lines, coral, colour, sepia, black and white or colour tile.
    pub photo_mode: usize,
    /// The last Check Design results (`embroidery.check` findings); `None` until run.
    pub checks: Option<Vec<serde_json::Value>>,
    /// The stitch type whose properties for new objects the Embroidery panel shows.
    pub new_type: String,
    /// Design from Prompt: the API key and the request on its way.
    pub prompt: prompt::PromptState,
    /// Ask About Embroidery: the conversation and the question on its way (NEW-07).
    pub ask: ask::AskState,
    /// Check My Sew-out: the photos and answers (SO-01).
    pub sewout: sewout::SewOutState,
    /// Foundry Account: the session and the credits (CLOUD-02).
    pub account: account::AccountState,
    /// Publish to MakerFoundry's window (PUB-01).
    pub publish: publish::PublishState,
    pub stitchshop: stitchshop::StitchShopState,
    pub family: family::FamilyState,
    /// Get Embroidery Fonts' window (NEW-20).
    pub font_library: crate::dialogs::font_library::FontLibraryState,
    /// Compare Designs' window (CMP-01).
    pub compare: crate::dialogs::compare::CompareState,
    /// The colour a tatami fill blends into (Make Blend in the Embroidery panel).
    pub blend_to: [u8; 3],
    /// The whole design shown repeated, columns across and rows down (1 × 1: once), with a
    /// gap between the copies (mm): a preview of a repeated layout, not sewn.
    pub repeats: [u32; 2],
    pub repeat_gap: f64,
    /// The Buttonholes section's settings.
    pub buttonholes: Buttonholes,
    /// The Longarm Quilting section's sizes (mm).
    pub quilt: QuiltSizes,
    /// The Embroidery Lettering section: the font, the text and its size (mm; 0: the font's own).
    pub lettering_font: String,
    pub lettering_text: String,
    pub lettering_size: f64,
    /// The selected lettering whose text, font and size fill the section (to change it).
    pub lettering_for: Option<u64>,
    /// The lettering's baseline round a circle of this radius (mm; positive arched, negative
    /// along its bottom, 0 straight).
    pub lettering_arc: f64,
    /// mm the arc's radius shrinks a turn (0: a circle).
    pub lettering_spiral: f64,
    /// The design notes as typed, while the field has the focus.
    pub notes_draft: Option<String>,
    /// Whether the section has asked for the fonts (loading the remembered folders) yet.
    pub fonts_listed: bool,
    /// The Team Names section's names, one a line.
    pub team_names: String,
    /// The Design Library section: its folder, search, sort and what it last found.
    pub library: LibraryView,
    /// The Batch Output section's choices.
    pub output: OutputView,
    /// The Patches section's choices.
    pub patches: PatchForm,
    /// A hoop of one's own as typed: name, width and height (mm).
    pub hoop_draft: (String, f64, f64),
    /// Recolour: the scheme (an index), the hue turn (degrees) and the one colour.
    pub recolor_scheme: usize,
    pub recolor_degrees: f64,
    pub recolor_tone: [u8; 3],
    /// The name typed to keep the design as a template.
    pub template_name: String,
    /// A new job order as typed: customer, quantity and due date.
    pub job_draft: (String, u32, String),
    /// When the send queue was last tried (UI time, seconds).
    pub queue_checked: f64,
    /// The lettering kiosk's screen.
    pub kiosk: kiosk::KioskView,
    /// The Monogram section: its letters, style (0 classic, 1 line), border (an index of
    /// `embroidery.monogram`'s borders) and size (mm; 0: the font's own).
    pub monogram_letters: String,
    pub monogram_style: usize,
    pub monogram_border: usize,
    pub monogram_size: f64,
    pub monogram_ornament: usize,
    /// The monogram template picked or being named.
    pub monogram_template: String,
    pub monogram_rings: usize,
    /// The Font Creator section: the font name as typed (and whether it is being typed), glyphs
    /// to add, a kerning pair to add, text to try the font with, and the type family and loaded
    /// font picked to start from or merge.
    pub font_name: String,
    pub font_name_editing: bool,
    pub font_add: String,
    pub kern_pair: String,
    pub font_test: String,
    pub font_family: String,
    pub font_other: String,
    /// The glyph the selected art becomes.
    pub font_assign: String,
    /// The order, joins and trims of the lettering to set.
    pub lettering_sew: vectorcraft_embroidery::sf_core::attr::LetterSewing,
    /// The letter of the selected lettering to move, scale or turn (from 0).
    pub letter_index: usize,
    /// The lettering to set is 3D puff.
    pub lettering_puff: bool,
    /// Appliqué letters in fabric of this colour.
    pub lettering_applique: Option<[u8; 3]>,
    /// How the letters are stitched.
    pub lettering_stitch: vectorcraft_embroidery::sf_core::attr::LetterStitch,
    /// Small-text settings.
    pub lettering_small: vectorcraft_embroidery::sf_core::attr::SmallText,
    /// Letters one under another.
    pub lettering_vertical: bool,
    /// The machine folder chosen to send to, and what the last send or check said.
    pub send_folder: usize,
    pub send_status: String,
}

/// The Longarm Quilting section's sizes (mm): the quilt for edge-to-edge rows, the rows' spacing
/// (0: the line's own height) and nesting, and a border's length, depth and corner leg (0: none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuiltSizes {
    pub width: f64,
    pub height: f64,
    pub row: f64,
    pub nest: bool,
    pub length: f64,
    pub depth: f64,
    pub down: f64,
}

impl Default for QuiltSizes {
    fn default() -> Self {
        // a twin quilt, 70 × 90″; a border 60″ long and 4″ deep
        QuiltSizes { width: 1778.0, height: 2286.0, row: 0.0, nest: true, length: 1524.0, depth: 101.6, down: 0.0 }
    }
}

/// Buttonholes to add from the Embroidery panel: one, or a row of them evenly spaced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buttonholes {
    /// 0 straight, 1 rounded, 2 keyhole.
    pub style: usize,
    /// The slit's length and the buttonhole's width (mm).
    pub length: f64,
    pub width: f64,
    pub count: u32,
    /// From one buttonhole's middle to the next (mm).
    pub spacing: f64,
    /// Degrees (0: the slits along x, the row down the page).
    pub angle: f64,
}

impl Default for Buttonholes {
    fn default() -> Self {
        Buttonholes { style: 0, length: 15.0, width: 4.0, count: 1, spacing: 25.0, angle: 0.0 }
    }
}

impl Buttonholes {
    /// `embroidery.addButtonholes` params for these buttonholes, the row centred on `centre`
    /// (points) and running square to the slits.
    pub fn params(&self, centre: vectorcraft_geom::Point) -> serde_json::Value {
        let style = ["straight", "rounded", "keyhole"].get(self.style).copied().unwrap_or("straight");
        let count = self.count.clamp(1, 50);
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let half = self.spacing * vectorcraft_embroidery::PT_PER_MM * f64::from(count - 1) * 0.5;
        let (dx, dy) = (-sin * half, cos * half);
        serde_json::json!({
            "style": style,
            "length": self.length,
            "width": self.width,
            "angle": self.angle,
            "count": count,
            "at": [centre.x - dx, centre.y - dy],
            "to": [centre.x + dx, centre.y + dy],
        })
    }
}

impl Default for EmbroideryUi {
    fn default() -> Self {
        EmbroideryUi {
            stitches: StitchView::TrueView,
            art: ArtView::Dimmed,
            connectors: false,
            needle_points: false,
            density_map: false,
            density: None,
            beads: true,
            guides: true,
            fabric: Color32::from_rgb(246, 243, 236),
            player: Player::default(),
            mesh: overlay::MeshCache::default(),
            widget: widget::WidgetState::default(),
            digitize_colors: 6,
            digitize_background: true,
            colour_preview: None,
            cutout_sensitivity: 50,
            cutout_preview: None,
            cutout_split: 0.5,
            photo_mode: 0,
            checks: None,
            new_type: "fill".into(),
            blend_to: [255, 255, 255],
            repeats: [1, 1],
            repeat_gap: 5.0,
            buttonholes: Buttonholes::default(),
            quilt: QuiltSizes::default(),
            lettering_font: String::new(),
            lettering_text: String::new(),
            lettering_size: 0.0,
            lettering_for: None,
            lettering_arc: 0.0,
            lettering_spiral: 0.0,
            notes_draft: None,
            fonts_listed: false,
            team_names: String::new(),
            library: LibraryView::default(),
            output: OutputView::default(),
            patches: PatchForm::default(),
            hoop_draft: (String::new(), 150.0, 150.0),
            recolor_scheme: 0,
            recolor_degrees: 60.0,
            recolor_tone: [0x1d, 0x35, 0x57],
            template_name: String::new(),
            job_draft: (String::new(), 1, String::new()),
            queue_checked: 0.0,
            kiosk: kiosk::KioskView::default(),
            monogram_letters: String::new(),
            monogram_style: 0,
            monogram_border: 1,
            monogram_size: 0.0,
            monogram_ornament: 0,
            monogram_template: String::new(),
            monogram_rings: 1,
            font_name: String::new(),
            font_name_editing: false,
            font_add: String::new(),
            kern_pair: String::new(),
            font_test: String::new(),
            font_family: "Source Sans 3".into(),
            font_other: String::new(),
            font_assign: String::new(),
            lettering_sew: vectorcraft_embroidery::sf_core::attr::LetterSewing::default(),
            letter_index: 0,
            lettering_puff: false,
            lettering_applique: None,
            lettering_stitch: Default::default(),
            lettering_small: Default::default(),
            lettering_vertical: false,
            send_folder: 0,
            send_status: String::new(),
            prompt: prompt::PromptState::default(),
            ask: Default::default(),
            sewout: Default::default(),
            account: Default::default(),
            publish: Default::default(),
            stitchshop: Default::default(),
            family: Default::default(),
            font_library: Default::default(),
            compare: Default::default(),
        }
    }
}

/// Advances the stitch player and opens the digitizing widget when the
/// embroidery brush is picked; call once per frame.
pub fn tick(app: &mut crate::VectorcraftApp, ctx: &egui::Context) {
    prompt::poll(app, ctx);
    ask::poll(app, ctx);
    account::poll(app, ctx);
    sewout::poll_report(app, ctx);
    publish::poll(app, ctx);
    stitchshop::poll(app, ctx);
    let tool = app.session.tool_id();
    if tool != app.embroidery.widget.last_tool {
        if tool == widget::TOOL {
            app.embroidery.widget.open = true;
        }
        app.embroidery.widget.last_tool = tool;
    }
    if !(app.embroidery.player.active && app.embroidery.player.playing) {
        return;
    }
    let total = app.session.embroidery_plan().map_or(0, |p| p.pattern().stitches.len()) as f64;
    let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
    let p = &mut app.embroidery.player;
    p.pos = (p.pos + p.speed * dt).min(total);
    if p.pos >= total {
        p.playing = false;
    }
    ctx.request_repaint();
}

/// An image reduced to a number of thread colours, as Auto-Digitize sees it.
/// Remove Background's preview of an image: the photo and its cut-out, small.
pub struct CutoutPreview {
    /// The image and the sensitivity it was made at.
    pub key: (u64, u32),
    pub before: egui::TextureHandle,
    pub after: egui::TextureHandle,
    /// The share of the image kept.
    pub kept: f64,
}

pub struct ColourPreview {
    /// The image, the colours asked for and whether the background is left out.
    pub key: (u64, u32, bool),
    pub texture: egui::TextureHandle,
    /// `embroidery.image.preview`'s answer (the colours, threads, thread changes).
    pub info: serde_json::Value,
}

/// The Design Library section's state.
#[derive(Default)]
pub struct LibraryView {
    pub folder: String,
    pub query: String,
    pub sort: String,
    /// `embroidery.library.scan`'s answer for them; none: to look.
    pub result: Option<serde_json::Value>,
    /// The format Convert writes, an index of the machine formats.
    pub convert_to: usize,
}

/// The Patches section's choices (`embroidery.patches.*`'s parameters).
pub struct PatchForm {
    /// An index of `PATCH_SHAPES`.
    pub shape: usize,
    pub width: f64,
    pub height: f64,
    /// The satin edge's width (mm, 0: none).
    pub border: f64,
    pub border_color: [u8; 3],
    /// The fill inside the edge (none: the fabric shows).
    pub background: Option<[u8; 3]>,
    pub text_color: [u8; 3],
    /// Words on every patch.
    pub text: String,
    /// One name a line: a patch each.
    pub names: String,
}

/// The shapes the Patches section offers, as `embroidery.patches.make` names them.
pub const PATCH_SHAPES: [&str; 5] = ["circle", "oval", "rectangle", "rounded", "shield"];

impl Default for PatchForm {
    fn default() -> Self {
        Self {
            shape: 0,
            width: 75.0,
            height: 75.0,
            border: 3.5,
            border_color: [0x1d, 0x35, 0x57],
            background: Some([0xf1, 0xfa, 0xee]),
            text_color: [0x1d, 0x35, 0x57],
            text: String::new(),
            names: String::new(),
        }
    }
}

/// The Batch Output section's state.
#[derive(Default)]
pub struct OutputView {
    /// The machine formats ticked (none: the design's machine's).
    pub formats: Vec<String>,
    /// The worksheet, an index of `OUTPUT_SHEETS`.
    pub worksheet: usize,
    /// Leave the design file out.
    pub no_design: bool,
    /// The profile chosen (0: none, else its index + 1) and the name typed to save one.
    pub profile: usize,
    pub profile_name: String,
    /// The folder last written to, and what was written.
    pub dir: String,
    pub status: String,
}
