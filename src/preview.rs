use std::fmt::Write;
use std::sync::OnceLock;

use base64::{Engine as _, engine::general_purpose};
use fontdb::{Family, Query, Style, Weight};
use slint::Image;
use ttf_parser::{Face, OutlineBuilder};

use crate::analysis;
use crate::domain::{EcgDocument, LeadData};
use crate::i18n::PageTexts;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageOrientation {
    Landscape,
    Portrait,
}

impl PageOrientation {
    pub fn from_key(key: &str) -> Self {
        match key {
            "portrait" => Self::Portrait,
            _ => Self::Landscape,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub orientation: PageOrientation,
    pub show_calibration: bool,
    pub grid_theme: GridTheme,
    pub clinic_logo: Option<ClinicLogo>,
    pub texts: PageTexts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridTheme {
    LightSalmon,
    TechnicalGray,
}

impl GridTheme {
    pub fn from_key(key: &str) -> Self {
        match key {
            "technical_gray" => Self::TechnicalGray,
            _ => Self::LightSalmon,
        }
    }

    fn palette(self) -> PagePalette {
        match self {
            Self::LightSalmon => PagePalette {
                background: Color::rgb(255, 255, 255),
                grid_minor: Color::rgb(246, 216, 204),
                grid_major: Color::rgb(232, 183, 164),
                panel_frame: Color::rgb(84, 68, 64),
                trace: Color::rgb(0, 0, 0),
                text: Color::rgb(17, 17, 17),
            },
            Self::TechnicalGray => PagePalette {
                background: Color::rgb(255, 255, 255),
                grid_minor: Color::rgb(230, 230, 230),
                grid_major: Color::rgb(200, 200, 200),
                panel_frame: Color::rgb(72, 72, 72),
                trace: Color::rgb(0, 0, 0),
                text: Color::rgb(17, 17, 17),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClinicLogo {
    pub display_name: String,
    data_uri: String,
    bitmap: Option<LogoBitmap>,
}

impl ClinicLogo {
    pub fn from_bytes(
        display_name: impl Into<String>,
        mime_type: &str,
        bytes: &[u8],
        bitmap: Option<LogoBitmap>,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            data_uri: format!(
                "data:{mime_type};base64,{}",
                general_purpose::STANDARD.encode(bytes)
            ),
            bitmap,
        }
    }

    fn aspect_ratio(&self) -> Option<f64> {
        self.bitmap.as_ref().and_then(|bitmap| {
            (bitmap.height > 0).then_some(bitmap.width as f64 / bitmap.height as f64)
        })
    }

    fn has_height_for(&self, rendered_height: f64) -> bool {
        self.bitmap
            .as_ref()
            .map(|bitmap| bitmap.height as f64 >= rendered_height.ceil())
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
pub struct LogoBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RenderedPage {
    pub width: f64,
    pub height: f64,
    pub commands: Vec<DrawCommand>,
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        stroke_width: f64,
    },
    Line {
        start: Point,
        end: Point,
        color: Color,
        stroke_width: f64,
    },
    Polyline {
        points: Vec<Point>,
        color: Color,
        stroke_width: f64,
    },
    Image {
        rect: Rect,
        data_uri: String,
        bitmap: Option<LogoBitmap>,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        font_size: f64,
        color: Color,
        weight: u16,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub(crate) fn width(self) -> f64 {
        self.right - self.left
    }

    pub(crate) fn height(self) -> f64 {
        self.bottom - self.top
    }

    fn inset(self, dx: f64, dy: f64) -> Self {
        Self {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right - dx,
            bottom: self.bottom - dy,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn svg(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

const CLINICAL_GRID: [[&str; 4]; 3] = [
    ["I", "aVR", "V1", "V4"],
    ["II", "aVL", "V2", "V5"],
    ["III", "aVF", "V3", "V6"],
];
const RHYTHM_LEAD: &str = "II";
const TRACE_SPEED_MM_PER_SECOND: f64 = 25.0;
const TRACE_GAIN_MM_PER_MV: f64 = 10.0;

const GRID_MINOR_STROKE: f64 = 0.45;
const GRID_MAJOR_STROKE: f64 = 0.8;
const FRAME_STROKE: f64 = 1.1;
const TRACE_STROKE: f64 = 0.85;

#[derive(Clone, Copy)]
struct PagePalette {
    background: Color,
    grid_minor: Color,
    grid_major: Color,
    panel_frame: Color,
    trace: Color,
    text: Color,
}

#[derive(Clone, Copy)]
struct PageScale {
    px_per_mm_x: f64,
    px_per_mm_y: f64,
}

impl PageScale {
    fn mm_x(self, value: f64) -> f64 {
        value * self.px_per_mm_x
    }

    fn mm_y(self, value: f64) -> f64 {
        value * self.px_per_mm_y
    }
}

impl RenderedPage {
    pub fn to_svg(&self) -> String {
        let mut svg = String::new();
        let _ = write!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {:.3} {:.3}" shape-rendering="geometricPrecision">"#,
            self.width, self.height, self.width, self.height
        );
        for command in &self.commands {
            match command {
                DrawCommand::FillRect { rect, color } => {
                    let _ = write!(
                        svg,
                        r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="{}"/>"#,
                        rect.left,
                        rect.top,
                        rect.width(),
                        rect.height(),
                        color.svg()
                    );
                }
                DrawCommand::StrokeRect {
                    rect,
                    color,
                    stroke_width,
                } => {
                    let _ = write!(
                        svg,
                        r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="none" stroke="{}" stroke-width="{:.3}"/>"#,
                        rect.left,
                        rect.top,
                        rect.width(),
                        rect.height(),
                        color.svg(),
                        stroke_width
                    );
                }
                DrawCommand::Line {
                    start,
                    end,
                    color,
                    stroke_width,
                } => {
                    let _ = write!(
                        svg,
                        r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke="{}" stroke-width="{:.3}" stroke-linecap="round"/>"#,
                        start.x,
                        start.y,
                        end.x,
                        end.y,
                        color.svg(),
                        stroke_width
                    );
                }
                DrawCommand::Polyline {
                    points,
                    color,
                    stroke_width,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    svg.push_str(r#"<polyline points=""#);
                    for point in points {
                        let _ = write!(svg, "{:.3},{:.3} ", point.x, point.y);
                    }
                    let _ = write!(
                        svg,
                        r#"" fill="none" stroke="{}" stroke-width="{:.3}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                        color.svg(),
                        stroke_width
                    );
                }
                DrawCommand::Image { rect, data_uri, .. } => {
                    let _ = write!(
                        svg,
                        r#"<image x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" href="{}" preserveAspectRatio="xMidYMid meet"/>"#,
                        rect.left,
                        rect.top,
                        rect.width(),
                        rect.height(),
                        data_uri
                    );
                }
                DrawCommand::Text {
                    x,
                    y,
                    text,
                    font_size,
                    color,
                    weight,
                } => write_vector_text(&mut svg, *x, *y, text, *font_size, *color, *weight),
            }
        }

        svg.push_str("</svg>");
        svg
    }
}

pub fn render_empty(
    orientation: PageOrientation,
    grid_theme: GridTheme,
    texts: PageTexts,
) -> Image {
    let mut document = EcgDocument::new(crate::domain::DocumentKind::Xml, Default::default());
    document.clinic_name = "ECG Studio".to_owned();
    render_document(
        &document,
        RenderOptions {
            orientation,
            show_calibration: true,
            grid_theme,
            clinic_logo: None,
            texts,
        },
    )
}

pub fn render_document(document: &EcgDocument, options: RenderOptions) -> Image {
    render_document_with_measurements(document, document, options)
}

pub fn render_document_with_measurements(
    document: &EcgDocument,
    measurements_document: &EcgDocument,
    options: RenderOptions,
) -> Image {
    let svg = render_document_svg_with_measurements(document, measurements_document, options);
    image_from_svg(&svg)
}

pub fn render_document_svg_with_measurements(
    document: &EcgDocument,
    measurements_document: &EcgDocument,
    options: RenderOptions,
) -> String {
    let page = render_document_page_with_measurements(document, measurements_document, options);
    page.to_svg()
}

pub fn image_from_svg(svg: &str) -> Image {
    Image::load_from_svg_data(svg.as_bytes()).unwrap_or_else(|_| fallback_image())
}

pub fn render_document_page_with_measurements(
    document: &EcgDocument,
    measurements_document: &EcgDocument,
    options: RenderOptions,
) -> RenderedPage {
    let (width, height, page_mm) = match options.orientation {
        PageOrientation::Landscape => (1600.0, 1131.0, (297.0, 210.0)),
        PageOrientation::Portrait => (1131.0, 1600.0, (210.0, 297.0)),
    };

    let palette = options.grid_theme.palette();
    let mut canvas = VectorCanvas::new(width, height, palette.background);
    let page = Rect {
        left: 0.0,
        top: 0.0,
        right: width,
        bottom: height,
    };

    let scale = PageScale {
        px_per_mm_x: page.width() / page_mm.0,
        px_per_mm_y: page.height() / page_mm.1,
    };

    let inner = page.inset(scale.mm_x(7.0), scale.mm_y(7.0));
    draw_header(
        &mut canvas,
        inner,
        scale,
        document,
        measurements_document,
        &options,
        palette,
    );

    let plot = Rect {
        left: inner.left,
        top: inner.top + scale.mm_y(38.0),
        right: inner.right,
        bottom: inner.bottom,
    };

    if has_clinical_layout(document) {
        draw_clinical_layout(
            &mut canvas,
            plot.inset(0.0, scale.mm_y(1.0)),
            scale,
            document,
            &options,
            palette,
        );
    } else {
        draw_fallback_layout(&mut canvas, plot, scale, document, &options, palette);
    }

    canvas.into_page()
}

fn draw_header(
    canvas: &mut VectorCanvas,
    inner: Rect,
    scale: PageScale,
    document: &EcgDocument,
    measurements_document: &EcgDocument,
    options: &RenderOptions,
    palette: PagePalette,
) {
    let body_font_size = scale.mm_y(4.1).max(15.0);
    let date_y = inner.top + scale.mm_y(25.0);
    let mut text_left = inner.left;
    if let Some(logo) = &options.clinic_logo {
        let default_logo_height = scale.mm_y(20.0).max(54.0);
        let target_logo_height = date_y + body_font_size - inner.top;
        let logo_height = if logo.has_height_for(target_logo_height) {
            target_logo_height
        } else {
            default_logo_height
        };
        let max_width = scale.mm_x(42.0).max(128.0);
        let logo_width = logo
            .aspect_ratio()
            .map(|ratio| (logo_height * ratio).min(max_width))
            .unwrap_or(logo_height);
        let rect = Rect {
            left: inner.left,
            top: inner.top,
            right: inner.left + logo_width,
            bottom: inner.top + logo_height,
        };
        canvas.image(rect, logo);
        text_left = rect.right + scale.mm_x(4.0);
    }

    let clinic = if document.clinic_name.trim().is_empty() {
        options.texts.clinic_fallback
    } else {
        document.clinic_name.trim()
    };
    canvas.draw_text(
        text_left,
        inner.top,
        clinic,
        scale.mm_y(6.0).max(23.0),
        palette.text,
        700,
    );

    let patient = if document.patient_name.trim().is_empty() {
        format!(
            "{}: {}",
            options.texts.patient, options.texts.edit_placeholder
        )
    } else {
        format!(
            "{}: {}",
            options.texts.patient,
            document.patient_name.trim()
        )
    };
    let date = if document.exam_date.trim().is_empty() {
        format!(
            "{}: {}",
            options.texts.exam_date, options.texts.edit_placeholder
        )
    } else {
        format!("{}: {}", options.texts.exam_date, document.exam_date.trim())
    };
    let birth = patient_birth_line(document, options.texts);
    let physician = if document.physician_name.trim().is_empty() {
        format!(
            "{}: {}",
            options.texts.physician, options.texts.edit_placeholder
        )
    } else {
        format!(
            "{}: {}",
            options.texts.physician,
            document.physician_name.trim()
        )
    };
    canvas.draw_text(
        text_left,
        inner.top + scale.mm_y(12.0),
        &patient,
        body_font_size,
        palette.text,
        400,
    );
    canvas.draw_text(
        text_left,
        inner.top + scale.mm_y(18.5),
        &birth,
        body_font_size,
        palette.text,
        400,
    );
    canvas.draw_text(text_left, date_y, &date, body_font_size, palette.text, 400);
    canvas.draw_text(
        text_left,
        inner.top + scale.mm_y(31.5),
        &physician,
        body_font_size,
        palette.text,
        400,
    );

    if !measurements_document.leads.is_empty() {
        draw_measurements_block(
            canvas,
            inner,
            scale,
            measurements_document,
            options,
            palette,
        );
    }
}

fn patient_birth_line(document: &EcgDocument, texts: PageTexts) -> String {
    let birth_date = if document.patient_birth_date.trim().is_empty() {
        texts.edit_placeholder
    } else {
        document.patient_birth_date.trim()
    };
    match document.patient_age_years() {
        Some(1) => format!(
            "{}: {birth_date} {}: 1 {}",
            texts.birth, texts.age, texts.year_singular
        ),
        Some(age) => format!(
            "{}: {birth_date} {}: {age} {}",
            texts.birth, texts.age, texts.year_plural
        ),
        None => format!("{}: {birth_date} {}: --", texts.birth, texts.age),
    }
}

fn draw_measurements_block(
    canvas: &mut VectorCanvas,
    inner: Rect,
    scale: PageScale,
    document: &EcgDocument,
    options: &RenderOptions,
    palette: PagePalette,
) {
    let block_width = scale.mm_x(98.0).max(360.0).min(inner.width() * 0.48);
    let mut y = inner.top;
    let line_size = scale.mm_y(2.8).max(10.0);
    let max_chars = (block_width / (line_size * 0.48)).floor().max(24.0) as usize;

    let mut lines =
        analysis::analyze_ecg(document).report_lines_with_texts(options.texts.measurements);
    if let Some(index) = lines.iter().position(|line| line.starts_with("QT:")) {
        let qt_line = lines.remove(index);
        lines.insert(0, qt_line);
    }

    let x = (inner.right - block_width).max(inner.left);
    for line in lines.into_iter().take(5) {
        let line = truncate_text(&line, max_chars);
        canvas.draw_text(x, y, &line, line_size, palette.text, 400);
        y += scale.mm_y(4.2).max(13.0);
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated: String = value.chars().take(max_chars.saturating_sub(3)).collect();
    truncated.push_str("...");
    truncated
}

fn has_clinical_layout(document: &EcgDocument) -> bool {
    CLINICAL_GRID
        .iter()
        .flatten()
        .all(|lead_name| document.find_lead(lead_name).is_some())
}

fn draw_clinical_layout(
    canvas: &mut VectorCanvas,
    plot: Rect,
    scale: PageScale,
    document: &EcgDocument,
    options: &RenderOptions,
    palette: PagePalette,
) {
    let column_gap = scale.mm_x(1.0).max(2.0);
    let row_gap = scale.mm_y(1.0).max(2.0);
    let footer_gap = scale.mm_y(0.8).max(1.0);
    let footer_height = scale.mm_y(5.0).max(10.0);
    let lead_area = Rect {
        left: plot.left,
        top: plot.top,
        right: plot.right,
        bottom: (plot.bottom - footer_gap - footer_height).max(plot.top + 4.0),
    };
    let footer = Rect {
        left: plot.left,
        top: lead_area.bottom + footer_gap,
        right: plot.right,
        bottom: plot.bottom,
    };
    let column_width = ((lead_area.width() - (column_gap * 3.0)) / 4.0).max(1.0);
    let row_height = ((lead_area.height() - (row_gap * 3.0)) / 4.0).max(1.0);

    for (row_index, row) in CLINICAL_GRID.iter().enumerate() {
        let context = LeadRenderContext {
            scale,
            sample_interval_seconds: document.sample_interval_seconds,
            show_tail: document.kind.is_live(),
            options,
            palette,
        };
        for (column_index, lead_name) in row.iter().enumerate() {
            let left = lead_area.left + column_index as f64 * (column_width + column_gap);
            let top = lead_area.top + row_index as f64 * (row_height + row_gap);
            let panel = Rect {
                left,
                top,
                right: if column_index == 3 {
                    lead_area.right
                } else {
                    left + column_width
                },
                bottom: top + row_height,
            };
            draw_lead_panel(
                canvas,
                panel,
                document.find_lead(lead_name),
                lead_name,
                &context,
            );
        }
    }

    let rhythm_top = lead_area.top + (3.0 * (row_height + row_gap));
    let rhythm = Rect {
        left: lead_area.left,
        top: rhythm_top,
        right: lead_area.right,
        bottom: lead_area.bottom,
    };
    draw_lead_panel(
        canvas,
        rhythm,
        document.find_lead(RHYTHM_LEAD),
        RHYTHM_LEAD,
        &LeadRenderContext {
            scale,
            sample_interval_seconds: document.sample_interval_seconds,
            show_tail: document.kind.is_live(),
            options,
            palette,
        },
    );
    let text_left = build_graph_rect(rhythm, scale, options.show_calibration).left;
    canvas.draw_text(
        text_left,
        footer.top + scale.mm_y(1.0),
        "25 mm/s   10 mm/mV",
        scale.mm_y(3.2).max(12.0),
        palette.text,
        400,
    );
}

fn draw_fallback_layout(
    canvas: &mut VectorCanvas,
    plot: Rect,
    scale: PageScale,
    document: &EcgDocument,
    options: &RenderOptions,
    palette: PagePalette,
) {
    if document.leads.is_empty() {
        canvas.draw_text(
            plot.left + scale.mm_x(8.0),
            plot.top + scale.mm_y(10.0),
            options.texts.no_leads,
            scale.mm_y(4.7).max(18.0),
            palette.text,
            400,
        );
        return;
    }

    let column_count = adaptive_column_count(document.leads.len(), options.orientation);
    let row_count = document.leads.len().div_ceil(column_count);
    let column_gap = scale.mm_x(1.0).max(2.0);
    let row_gap = scale.mm_y(1.0).max(2.0);
    let footer_gap = scale.mm_y(0.8).max(1.0);
    let footer_height = scale.mm_y(5.0).max(10.0);
    let lead_area = Rect {
        left: plot.left,
        top: plot.top,
        right: plot.right,
        bottom: (plot.bottom - footer_gap - footer_height).max(plot.top + 4.0),
    };
    let footer = Rect {
        left: plot.left,
        top: lead_area.bottom + footer_gap,
        right: plot.right,
        bottom: plot.bottom,
    };
    let column_width = ((lead_area.width() - column_gap * (column_count.saturating_sub(1) as f64))
        / column_count as f64)
        .max(1.0);
    let row_height = ((lead_area.height() - row_gap * (row_count.saturating_sub(1) as f64))
        / row_count as f64)
        .max(1.0);
    let context = LeadRenderContext {
        scale,
        sample_interval_seconds: document.sample_interval_seconds,
        show_tail: document.kind.is_live(),
        options,
        palette,
    };

    for (index, lead) in document.leads.iter().enumerate() {
        let row_index = index / column_count;
        let column_index = index % column_count;
        let left = lead_area.left + column_index as f64 * (column_width + column_gap);
        let top = lead_area.top + row_index as f64 * (row_height + row_gap);
        let panel = Rect {
            left,
            top,
            right: if column_index + 1 == column_count {
                lead_area.right
            } else {
                left + column_width
            },
            bottom: if row_index + 1 == row_count {
                lead_area.bottom
            } else {
                top + row_height
            },
        };
        draw_lead_panel(canvas, panel, Some(lead), &lead.name, &context);
    }

    canvas.draw_text(
        plot.left + scale.mm_x(1.5),
        footer.top + scale.mm_y(1.0),
        "25 mm/s   10 mm/mV",
        scale.mm_y(3.2).max(12.0),
        palette.text,
        400,
    );
}

fn adaptive_column_count(lead_count: usize, orientation: PageOrientation) -> usize {
    match orientation {
        PageOrientation::Landscape if lead_count <= 3 => 1,
        PageOrientation::Landscape if lead_count <= 6 => 2,
        PageOrientation::Landscape if lead_count <= 9 => 3,
        PageOrientation::Landscape => 4,
        PageOrientation::Portrait if lead_count <= 4 => 1,
        PageOrientation::Portrait if lead_count <= 8 => 2,
        PageOrientation::Portrait => 3,
    }
}

struct LeadRenderContext<'a> {
    scale: PageScale,
    sample_interval_seconds: f64,
    show_tail: bool,
    options: &'a RenderOptions,
    palette: PagePalette,
}

fn draw_lead_panel(
    canvas: &mut VectorCanvas,
    panel: Rect,
    lead: Option<&LeadData>,
    label: &str,
    context: &LeadRenderContext<'_>,
) {
    draw_grid(canvas, panel, context.scale, context.palette);
    canvas.stroke_rect(panel, context.palette.panel_frame, FRAME_STROKE);
    canvas.draw_text(
        panel.left + context.scale.mm_x(1.5),
        panel.top + context.scale.mm_y(1.2),
        label,
        context.scale.mm_y(3.4).max(12.0),
        context.palette.text,
        700,
    );

    if context.options.show_calibration {
        draw_calibration_pulse(canvas, panel, context.scale, context.palette);
    }

    let Some(lead) = lead else {
        return;
    };
    let graph = build_graph_rect(panel, context.scale, context.options.show_calibration);
    let sample_count =
        samples_for_graph_width(graph, context.scale, context.sample_interval_seconds)
            .min(lead.samples_microvolts.len());
    draw_signal(canvas, panel, graph, lead, sample_count, context);
}

fn draw_grid(canvas: &mut VectorCanvas, rect: Rect, scale: PageScale, palette: PagePalette) {
    let step_x = scale.mm_x(1.0).max(1.0);
    let step_y = scale.mm_y(1.0).max(1.0);

    let mut x = rect.left;
    let mut column = 0;
    while x <= rect.right {
        let is_major = column % 5 == 0;
        canvas.line(
            Point { x, y: rect.top },
            Point { x, y: rect.bottom },
            if is_major {
                palette.grid_major
            } else {
                palette.grid_minor
            },
            if is_major {
                GRID_MAJOR_STROKE
            } else {
                GRID_MINOR_STROKE
            },
        );
        x += step_x;
        column += 1;
    }

    let mut y = rect.top;
    let mut row = 0;
    while y <= rect.bottom {
        let is_major = row % 5 == 0;
        canvas.line(
            Point { x: rect.left, y },
            Point { x: rect.right, y },
            if is_major {
                palette.grid_major
            } else {
                palette.grid_minor
            },
            if is_major {
                GRID_MAJOR_STROKE
            } else {
                GRID_MINOR_STROKE
            },
        );
        y += step_y;
        row += 1;
    }
}

fn draw_calibration_pulse(
    canvas: &mut VectorCanvas,
    panel: Rect,
    scale: PageScale,
    palette: PagePalette,
) {
    let baseline = (panel.top + panel.bottom) / 2.0;
    let x0 = panel.left + scale.mm_x(1.0);
    let x1 = x0 + scale.mm_x(1.0);
    let x2 = x1 + scale.mm_x(5.0);
    let x3 = x2 + scale.mm_x(1.0);
    let top = baseline - scale.mm_y(10.0);
    canvas.polyline(
        vec![
            Point { x: x0, y: baseline },
            Point { x: x1, y: baseline },
            Point { x: x1, y: top },
            Point { x: x2, y: top },
            Point { x: x2, y: baseline },
            Point { x: x3, y: baseline },
        ],
        palette.trace,
        TRACE_STROKE,
    );
}

fn build_graph_rect(panel: Rect, scale: PageScale, show_calibration: bool) -> Rect {
    Rect {
        left: if show_calibration {
            panel.left + scale.mm_x(9.5)
        } else {
            panel.left + scale.mm_x(1.8)
        },
        top: panel.top + scale.mm_y(1.0),
        right: panel.right - scale.mm_x(1.5),
        bottom: panel.bottom - scale.mm_y(1.0),
    }
}

fn samples_for_graph_width(graph: Rect, scale: PageScale, sample_interval_seconds: f64) -> usize {
    if sample_interval_seconds <= 0.0 || scale.px_per_mm_x <= 0.0 || graph.width() <= 0.0 {
        return 0;
    }
    let width_mm = graph.width() / scale.px_per_mm_x;
    let duration = width_mm / TRACE_SPEED_MM_PER_SECOND;
    ((duration / sample_interval_seconds).floor() as usize).saturating_add(1)
}

fn draw_signal(
    canvas: &mut VectorCanvas,
    panel: Rect,
    graph: Rect,
    lead: &LeadData,
    sample_count: usize,
    context: &LeadRenderContext<'_>,
) {
    if sample_count < 2 || context.sample_interval_seconds <= 0.0 {
        return;
    }

    let sample_offset = if context.show_tail {
        lead.samples_microvolts.len().saturating_sub(sample_count)
    } else {
        0
    };
    let samples = &lead.samples_microvolts[sample_offset..sample_offset + sample_count];
    let baseline = (panel.top + panel.bottom) / 2.0;
    let amplitude_scale = context.scale.px_per_mm_y * (TRACE_GAIN_MM_PER_MV / 1000.0);
    let time_scale =
        context.scale.px_per_mm_x * TRACE_SPEED_MM_PER_SECOND * context.sample_interval_seconds;
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let width = graph.width().max(1.0);
    let bucket = ((sample_count as f64 / width).ceil() as usize).max(1);
    let mut points = Vec::new();

    for start in (0..sample_count).step_by(bucket) {
        let end = (start + bucket).min(sample_count);
        for index in representative_indices(samples, start, end) {
            let x = graph.left + index as f64 * time_scale;
            if x > graph.right {
                continue;
            }
            let sample = samples[index];
            let y = baseline - ((sample - mean) * amplitude_scale);
            points.push(Point {
                x,
                y: y.clamp(graph.top, graph.bottom),
            });
        }
    }

    canvas.polyline(points, context.palette.trace, TRACE_STROKE);
}

fn representative_indices(samples: &[f64], start: usize, end: usize) -> Vec<usize> {
    if end <= start + 1 {
        return vec![start];
    }

    let mut min_index = start;
    let mut max_index = start;
    for index in (start + 1)..end {
        if samples[index] < samples[min_index] {
            min_index = index;
        }
        if samples[index] > samples[max_index] {
            max_index = index;
        }
    }

    let mut indices = vec![start, min_index, max_index, end - 1];
    indices.sort_unstable();
    indices.dedup();
    indices
}

struct VectorCanvas {
    width: f64,
    height: f64,
    commands: Vec<DrawCommand>,
}

impl VectorCanvas {
    fn new(width: f64, height: f64, background: Color) -> Self {
        let mut canvas = Self {
            width,
            height,
            commands: Vec::new(),
        };
        canvas.fill_rect(
            Rect {
                left: 0.0,
                top: 0.0,
                right: width,
                bottom: height,
            },
            background,
        );
        canvas
    }

    fn into_page(self) -> RenderedPage {
        RenderedPage {
            width: self.width,
            height: self.height,
            commands: self.commands,
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.commands.push(DrawCommand::FillRect { rect, color });
    }

    fn stroke_rect(&mut self, rect: Rect, color: Color, stroke_width: f64) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            color,
            stroke_width,
        });
    }

    fn line(&mut self, start: Point, end: Point, color: Color, stroke_width: f64) {
        self.commands.push(DrawCommand::Line {
            start,
            end,
            color,
            stroke_width,
        });
    }

    fn polyline(&mut self, points: Vec<Point>, color: Color, stroke_width: f64) {
        if points.len() >= 2 {
            self.commands.push(DrawCommand::Polyline {
                points,
                color,
                stroke_width,
            });
        }
    }

    fn image(&mut self, rect: Rect, logo: &ClinicLogo) {
        self.commands.push(DrawCommand::Image {
            rect,
            data_uri: logo.data_uri.clone(),
            bitmap: logo.bitmap.clone(),
        });
    }

    fn draw_text(&mut self, x: f64, y: f64, text: &str, font_size: f64, color: Color, weight: u16) {
        self.commands.push(DrawCommand::Text {
            x,
            y,
            text: text.to_owned(),
            font_size,
            color,
            weight,
        });
    }
}

fn fallback_image() -> Image {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1131" viewBox="0 0 1600 1131"><rect width="1600" height="1131" fill="#ffffff"/></svg>"##;
    Image::load_from_svg_data(svg.as_bytes()).expect("fallback SVG must be valid")
}

fn write_vector_text(
    svg: &mut String,
    x: f64,
    y: f64,
    text: &str,
    font_size: f64,
    color: Color,
    weight: u16,
) {
    if write_system_font_text(svg, x, y, text, font_size, color, weight) {
        return;
    }

    write_fallback_vector_text(svg, x, y, text, font_size, color, weight);
}

fn write_system_font_text(
    svg: &mut String,
    x: f64,
    y: f64,
    text: &str,
    font_size: f64,
    color: Color,
    weight: u16,
) -> bool {
    let Some(font) = system_text_font(weight) else {
        return false;
    };
    let Ok(face) = Face::parse(&font.data, font.index) else {
        return false;
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em <= 0.0 {
        return false;
    }

    let scale = font_size / units_per_em;
    let baseline = y + face.ascender() as f64 * scale;
    let mut cursor = x;
    let paint = color.svg();
    let mut wrote = false;

    let _ = write!(svg, r#"<g fill="{}">"#, paint);
    for ch in text.chars() {
        if ch.is_whitespace() {
            cursor += font_size * 0.35;
            continue;
        }

        let Some(glyph_id) = face.glyph_index(ch) else {
            cursor += font_size * 0.35;
            continue;
        };

        let mut path = SvgGlyphPath::new(cursor, baseline, scale);
        if face.outline_glyph(glyph_id, &mut path).is_some() && !path.data.is_empty() {
            let _ = write!(svg, r#"<path d="{}"/>"#, path.data);
            wrote = true;
        }

        cursor += face
            .glyph_hor_advance(glyph_id)
            .map(|advance| advance as f64 * scale)
            .unwrap_or(font_size * 0.55);
    }
    svg.push_str("</g>");

    wrote
}

fn write_fallback_vector_text(
    svg: &mut String,
    x: f64,
    y: f64,
    text: &str,
    font_size: f64,
    color: Color,
    weight: u16,
) {
    let scale = (font_size / 7.0).max(1.0);
    let stroke = scale * if weight >= 600 { 0.62 } else { 0.48 };
    let dot_radius = stroke * 0.42;
    let mut cursor = x;
    let paint = color.svg();

    let _ = write!(
        svg,
        r#"<g stroke="{}" fill="none" stroke-linecap="round" stroke-linejoin="round" stroke-width="{:.3}">"#,
        paint, stroke
    );
    for ch in text.chars() {
        if ch.is_whitespace() {
            cursor += scale * 4.0;
            continue;
        }

        let Some(pattern) = glyph(ch) else {
            cursor += scale * 4.0;
            continue;
        };

        write_glyph_runs(svg, pattern, cursor, y, scale, dot_radius, &paint);
        cursor += scale * 6.0;
    }
    svg.push_str("</g>");
}

#[derive(Clone)]
struct FontFaceData {
    data: Vec<u8>,
    index: u32,
}

fn system_text_font(weight: u16) -> Option<&'static FontFaceData> {
    static REGULAR: OnceLock<Option<FontFaceData>> = OnceLock::new();
    static BOLD: OnceLock<Option<FontFaceData>> = OnceLock::new();
    let slot = if weight >= 600 { &BOLD } else { &REGULAR };
    slot.get_or_init(|| load_system_text_font(weight)).as_ref()
}

fn load_system_text_font(weight: u16) -> Option<FontFaceData> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let families = [
        Family::Name("Segoe UI Variable Text"),
        Family::Name("Segoe UI"),
        Family::Name("DejaVu Sans"),
        Family::Name("Noto Sans"),
        Family::Name("Liberation Sans"),
        Family::SansSerif,
    ];
    let id = database.query(&Query {
        families: &families,
        weight: if weight >= 600 {
            Weight::BOLD
        } else {
            Weight::NORMAL
        },
        stretch: Default::default(),
        style: Style::Normal,
    })?;

    database.with_face_data(id, |data, index| FontFaceData {
        data: data.to_vec(),
        index,
    })
}

struct SvgGlyphPath {
    data: String,
    offset_x: f64,
    baseline: f64,
    scale: f64,
}

impl SvgGlyphPath {
    fn new(offset_x: f64, baseline: f64, scale: f64) -> Self {
        Self {
            data: String::new(),
            offset_x,
            baseline,
            scale,
        }
    }

    fn x(&self, value: f32) -> f64 {
        self.offset_x + value as f64 * self.scale
    }

    fn y(&self, value: f32) -> f64 {
        self.baseline - value as f64 * self.scale
    }
}

impl OutlineBuilder for SvgGlyphPath {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.data, "M{:.3},{:.3}", self.x(x), self.y(y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.data, "L{:.3},{:.3}", self.x(x), self.y(y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(
            self.data,
            "Q{:.3},{:.3} {:.3},{:.3}",
            self.x(x1),
            self.y(y1),
            self.x(x),
            self.y(y)
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(
            self.data,
            "C{:.3},{:.3} {:.3},{:.3} {:.3},{:.3}",
            self.x(x1),
            self.y(y1),
            self.x(x2),
            self.y(y2),
            self.x(x),
            self.y(y)
        );
    }

    fn close(&mut self) {
        self.data.push('Z');
    }
}

fn write_glyph_runs(
    svg: &mut String,
    pattern: [&'static str; 7],
    x: f64,
    y: f64,
    scale: f64,
    dot_radius: f64,
    paint: &str,
) {
    for (row, bits) in pattern.iter().enumerate() {
        let columns: Vec<char> = bits.chars().collect();
        let mut col = 0;
        while col < columns.len() {
            if columns[col] != '1' {
                col += 1;
                continue;
            }
            let start = col;
            while col + 1 < columns.len() && columns[col + 1] == '1' {
                col += 1;
            }
            if col > start {
                let yy = y + (row as f64 + 0.5) * scale;
                let _ = write!(
                    svg,
                    r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}"/>"#,
                    x + (start as f64 + 0.5) * scale,
                    yy,
                    x + (col as f64 + 0.5) * scale,
                    yy
                );
            }
            col += 1;
        }
    }

    for col in 0..5 {
        let mut row = 0;
        while row < pattern.len() {
            let is_on = pattern[row].as_bytes().get(col).copied() == Some(b'1');
            if !is_on {
                row += 1;
                continue;
            }
            let start = row;
            while row + 1 < pattern.len()
                && pattern[row + 1].as_bytes().get(col).copied() == Some(b'1')
            {
                row += 1;
            }
            if row > start {
                let xx = x + (col as f64 + 0.5) * scale;
                let _ = write!(
                    svg,
                    r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}"/>"#,
                    xx,
                    y + (start as f64 + 0.5) * scale,
                    xx,
                    y + (row as f64 + 0.5) * scale
                );
            } else {
                let _ = write!(
                    svg,
                    r#"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="{}" stroke="none"/>"#,
                    x + (col as f64 + 0.5) * scale,
                    y + (row as f64 + 0.5) * scale,
                    dot_radius,
                    paint
                );
            }
            row += 1;
        }
    }
}

fn glyph(ch: char) -> Option<[&'static str; 7]> {
    match normalize_glyph_char(ch) {
        'A' => Some([
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ]),
        'B' => Some([
            "11110", "10001", "10001", "11110", "10001", "10001", "11110",
        ]),
        'C' => Some([
            "01111", "10000", "10000", "10000", "10000", "10000", "01111",
        ]),
        'D' => Some([
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ]),
        'E' => Some([
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ]),
        'F' => Some([
            "11111", "10000", "10000", "11110", "10000", "10000", "10000",
        ]),
        'G' => Some([
            "01111", "10000", "10000", "10111", "10001", "10001", "01111",
        ]),
        'H' => Some([
            "10001", "10001", "10001", "11111", "10001", "10001", "10001",
        ]),
        'I' => Some([
            "11111", "00100", "00100", "00100", "00100", "00100", "11111",
        ]),
        'J' => Some([
            "00111", "00010", "00010", "00010", "10010", "10010", "01100",
        ]),
        'K' => Some([
            "10001", "10010", "10100", "11000", "10100", "10010", "10001",
        ]),
        'L' => Some([
            "10000", "10000", "10000", "10000", "10000", "10000", "11111",
        ]),
        'M' => Some([
            "10001", "11011", "10101", "10101", "10001", "10001", "10001",
        ]),
        'N' => Some([
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ]),
        'O' => Some([
            "01110", "10001", "10001", "10001", "10001", "10001", "01110",
        ]),
        'P' => Some([
            "11110", "10001", "10001", "11110", "10000", "10000", "10000",
        ]),
        'Q' => Some([
            "01110", "10001", "10001", "10001", "10101", "10010", "01101",
        ]),
        'R' => Some([
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ]),
        'S' => Some([
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ]),
        'T' => Some([
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ]),
        'U' => Some([
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ]),
        'V' => Some([
            "10001", "10001", "10001", "10001", "10001", "01010", "00100",
        ]),
        'W' => Some([
            "10001", "10001", "10001", "10101", "10101", "10101", "01010",
        ]),
        'X' => Some([
            "10001", "10001", "01010", "00100", "01010", "10001", "10001",
        ]),
        'Y' => Some([
            "10001", "10001", "01010", "00100", "00100", "00100", "00100",
        ]),
        'Z' => Some([
            "11111", "00001", "00010", "00100", "01000", "10000", "11111",
        ]),
        '0' => Some([
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ]),
        '1' => Some([
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ]),
        '2' => Some([
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ]),
        '3' => Some([
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ]),
        '4' => Some([
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ]),
        '5' => Some([
            "11111", "10000", "10000", "11110", "00001", "00001", "11110",
        ]),
        '6' => Some([
            "01110", "10000", "10000", "11110", "10001", "10001", "01110",
        ]),
        '7' => Some([
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ]),
        '8' => Some([
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ]),
        '9' => Some([
            "01110", "10001", "10001", "01111", "00001", "00001", "01110",
        ]),
        ':' => Some([
            "00000", "00100", "00100", "00000", "00100", "00100", "00000",
        ]),
        '/' => Some([
            "00001", "00010", "00010", "00100", "01000", "01000", "10000",
        ]),
        '-' => Some([
            "00000", "00000", "00000", "11111", "00000", "00000", "00000",
        ]),
        '(' => Some([
            "00010", "00100", "01000", "01000", "01000", "00100", "00010",
        ]),
        ')' => Some([
            "01000", "00100", "00010", "00010", "00010", "00100", "01000",
        ]),
        '.' => Some([
            "00000", "00000", "00000", "00000", "00000", "01100", "01100",
        ]),
        ',' => Some([
            "00000", "00000", "00000", "00000", "00000", "01100", "00100",
        ]),
        _ => None,
    }
}

fn normalize_glyph_char(ch: char) -> char {
    match ch.to_ascii_uppercase() {
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'á' | 'à' | 'â' | 'ã' | 'ä' => 'A',
        'Ç' | 'ç' => 'C',
        'É' | 'È' | 'Ê' | 'Ë' | 'é' | 'è' | 'ê' | 'ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' | 'í' | 'ì' | 'î' | 'ï' => 'I',
        'Ñ' | 'ñ' => 'N',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' | 'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' | 'ú' | 'ù' | 'û' | 'ü' => 'U',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DocumentKind, LeadData};

    #[test]
    fn renders_valid_svg_with_vector_trace() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new(
            "II",
            (0..1000)
                .map(|index| ((index as f64) / 18.0).sin() * 850.0)
                .collect(),
        ));

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );
        let svg = page.to_svg();

        assert!(svg.contains("<polyline"));
        assert!(svg.contains("#f6d8cc"));
        Image::load_from_svg_data(svg.as_bytes()).expect("generated SVG should load");
    }

    #[test]
    fn uses_darker_salmon_frame_without_changing_grid_color() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new("II", vec![0.0; 1_000]));

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );
        let palette = GridTheme::LightSalmon.palette();

        assert_ne!(palette.panel_frame, palette.grid_major);
        assert!(page.commands.iter().any(|command| matches!(
            command,
            DrawCommand::StrokeRect { color, .. } if *color == palette.panel_frame
        )));
        assert!(page.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Line { color, .. } if *color == palette.grid_major
        )));
    }

    #[test]
    fn plots_signal_using_ecg_paper_scale() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.sample_interval_seconds = 0.04;
        document.leads.push(LeadData::new("II", vec![0.0, 1_000.0]));

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: false,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );

        let points = page
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Polyline { points, .. } => Some(points),
                _ => None,
            })
            .expect("signal polyline should be rendered");

        assert_eq!(points.len(), 2);
        assert_close(points[1].x - points[0].x, page.width / 297.0);
        assert_close(
            (points[0].y - points[1].y).abs(),
            (page.height / 210.0) * 10.0,
        );
    }

    #[test]
    fn renders_measurements_from_the_explicit_measurements_document() {
        let mut displayed = EcgDocument::new(DocumentKind::Xml, Default::default());
        displayed.leads.push(LeadData::new("II", vec![0.0; 1_000]));
        let measurements = EcgDocument::new(DocumentKind::Xml, Default::default());

        let page = render_document_page_with_measurements(
            &displayed,
            &measurements,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );

        assert!(!page.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.starts_with("FC:")
        )));
    }

    #[test]
    fn left_aligns_measurements_inside_the_upper_right_block() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new("II", vec![0.0; 1_000]));

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );

        let measurement_positions: Vec<(f64, f64)> = page
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { x, y, text, .. }
                    if text.starts_with("QT:")
                        || text.starts_with("FC:")
                        || text.starts_with("Faixa:")
                        || text.starts_with("PR:")
                        || text.starts_with("Eixo QRS:") =>
                {
                    Some((*x, *y))
                }
                _ => None,
            })
            .collect();

        let (first_x, first_y) = measurement_positions
            .first()
            .copied()
            .expect("measurement lines should be rendered");
        assert!(first_x > page.width * 0.55);
        assert!(first_y < 90.0);
        assert!(
            measurement_positions
                .iter()
                .all(|(x, _)| (*x - first_x).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn arranges_partial_lead_sets_in_an_adaptive_grid() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        for lead_name in ["I", "II", "III", "V1", "V2", "V3"] {
            document
                .leads
                .push(LeadData::new(lead_name, vec![0.0; 1_000]));
        }

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: None,
                texts: test_page_texts(),
            },
        );

        let lead_names = ["I", "II", "III", "V1", "V2", "V3"];
        let positions = page
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { x, y, text, .. } if lead_names.contains(&text.as_str()) => {
                    Some((*x, *y))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut x_positions = positions
            .iter()
            .map(|(x, _)| (*x / 10.0).round() as i32)
            .collect::<Vec<_>>();
        x_positions.sort_unstable();
        x_positions.dedup();

        let mut y_positions = positions
            .iter()
            .map(|(_, y)| (*y / 10.0).round() as i32)
            .collect::<Vec<_>>();
        y_positions.sort_unstable();
        y_positions.dedup();

        assert_eq!(positions.len(), 6);
        assert_eq!(x_positions.len(), 2);
        assert_eq!(y_positions.len(), 3);
    }

    #[test]
    fn grows_logo_to_bottom_of_exam_date_when_resolution_allows() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.exam_date = "11/05/2026".to_owned();

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: Some(test_logo(400, 400)),
                texts: test_page_texts(),
            },
        );

        let logo = first_logo_rect(&page);
        let (date_y, date_font_size) = text_position(&page, "Data do exame:");

        assert_close(logo.bottom, date_y + date_font_size);
    }

    #[test]
    fn keeps_logo_default_height_when_target_resolution_is_missing() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.exam_date = "11/05/2026".to_owned();

        let page = render_document_page_with_measurements(
            &document,
            &document,
            RenderOptions {
                orientation: PageOrientation::Landscape,
                show_calibration: true,
                grid_theme: GridTheme::LightSalmon,
                clinic_logo: Some(test_logo(80, 80)),
                texts: test_page_texts(),
            },
        );

        let logo = first_logo_rect(&page);
        let (_, date_font_size) = text_position(&page, "Data do exame:");
        let default_height = (page.height / 210.0 * 20.0).max(54.0);

        assert_close(logo.height(), default_height);
        assert!(logo.height() < (page.height / 210.0 * 25.0) + date_font_size);
    }

    #[test]
    fn formats_birth_and_age_without_separator_bar() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.patient_birth_date = "10/05/1980".to_owned();
        document.exam_date = "11/05/2026".to_owned();

        let line = patient_birth_line(&document, test_page_texts());

        assert_eq!(line, "Nascimento: 10/05/1980 Idade: 46 anos");
        assert!(!line.contains('|'));
    }

    fn test_logo(width: u32, height: u32) -> ClinicLogo {
        ClinicLogo::from_bytes(
            "logo.png",
            "image/png",
            &[0],
            Some(LogoBitmap {
                width,
                height,
                rgba: Vec::new(),
            }),
        )
    }

    fn test_page_texts() -> PageTexts {
        crate::i18n::texts(crate::i18n::Language::PtBr).page
    }

    fn first_logo_rect(page: &RenderedPage) -> Rect {
        page.commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("logo should be rendered")
    }

    fn text_position(page: &RenderedPage, prefix: &str) -> (f64, f64) {
        page.commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text {
                    y, text, font_size, ..
                } if text.starts_with(prefix) => Some((*y, *font_size)),
                _ => None,
            })
            .expect("text should be rendered")
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {actual} to be close to {expected}"
        );
    }
}
