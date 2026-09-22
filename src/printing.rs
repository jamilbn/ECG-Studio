#[cfg(any(target_os = "linux", test))]
use std::fmt::Write;

use crate::preview::RenderedPage;
#[cfg(any(windows, target_os = "linux", test))]
use crate::preview::{Color, DrawCommand, LogoBitmap, Point, Rect};

pub fn print_page(page: &RenderedPage) -> Result<(), String> {
    print_page_platform(page)
}

#[cfg(windows)]
fn print_page_platform(page: &RenderedPage) -> Result<(), String> {
    use std::mem::size_of;

    use windows::Win32::Graphics::Gdi::DeleteDC;
    use windows::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, PD_DISABLEPRINTTOFILE, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC,
        PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW, PrintDlgW,
    };

    if page.width <= 0.0 || page.height <= 0.0 || page.commands.is_empty() {
        return Err("Não há ECG vetorial para imprimir.".to_owned());
    }

    let mut dialog = PRINTDLGW {
        lStructSize: size_of::<PRINTDLGW>() as u32,
        hDevMode: default_print_dialog_dev_mode(page),
        Flags: PD_RETURNDC
            | PD_NOSELECTION
            | PD_NOPAGENUMS
            | PD_DISABLEPRINTTOFILE
            | PD_USEDEVMODECOPIESANDCOLLATE,
        nCopies: 1,
        ..Default::default()
    };

    if !unsafe { PrintDlgW(&mut dialog) }.as_bool() {
        let error = unsafe { CommDlgExtendedError() };
        free_print_dialog_handles(dialog.hDevMode, dialog.hDevNames);
        if error.0 == 0 {
            return Err("Impressão cancelada.".to_owned());
        }
        return Err(format!(
            "Falha ao abrir o diálogo de impressão: código 0x{:04X}.",
            error.0
        ));
    }

    let hdc = dialog.hDC;
    if hdc.0.is_null() {
        free_print_dialog_handles(dialog.hDevMode, dialog.hDevNames);
        return Err("A impressora não retornou um contexto de desenho válido.".to_owned());
    }

    let result = print_to_hdc(hdc, page);

    unsafe {
        let _ = DeleteDC(hdc);
    }
    free_print_dialog_handles(dialog.hDevMode, dialog.hDevNames);

    result
}

#[cfg(windows)]
fn default_print_dialog_dev_mode(page: &RenderedPage) -> windows::Win32::Foundation::HGLOBAL {
    use windows::Win32::Foundation::{GlobalFree, HGLOBAL};
    use windows::Win32::Graphics::Gdi::{
        DEVMODE_FIELD_FLAGS, DEVMODEW, DM_IN_BUFFER, DM_ORIENTATION, DM_OUT_BUFFER,
        DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT,
    };
    use windows::Win32::Graphics::Printing::{
        ClosePrinter, DocumentPropertiesW, GetDefaultPrinterW, OpenPrinterW, PRINTER_HANDLE,
    };
    use windows::Win32::System::Memory::{
        GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock,
    };
    use windows::core::{PCWSTR, PWSTR};

    const DOCUMENT_PROPERTIES_OK: i32 = 1;

    let mut printer_name_len = 0u32;
    let _ = unsafe { GetDefaultPrinterW(None, &mut printer_name_len) };
    if printer_name_len == 0 {
        return HGLOBAL::default();
    }

    let mut printer_name = vec![0u16; printer_name_len as usize];
    if !unsafe {
        GetDefaultPrinterW(
            Some(PWSTR(printer_name.as_mut_ptr())),
            &mut printer_name_len,
        )
    }
    .as_bool()
    {
        return HGLOBAL::default();
    }

    let printer_name = PCWSTR(printer_name.as_ptr());
    let mut printer = PRINTER_HANDLE::default();
    if unsafe { OpenPrinterW(printer_name, &mut printer, None) }.is_err() {
        return HGLOBAL::default();
    }

    let required_bytes = unsafe { DocumentPropertiesW(None, printer, printer_name, None, None, 0) };
    if required_bytes <= 0 {
        let _ = unsafe { ClosePrinter(printer) };
        return HGLOBAL::default();
    }

    let dev_mode =
        match unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, required_bytes as usize) } {
            Ok(handle) => handle,
            Err(_) => {
                let _ = unsafe { ClosePrinter(printer) };
                return HGLOBAL::default();
            }
        };

    let raw_dev_mode = unsafe { GlobalLock(dev_mode) };
    if raw_dev_mode.is_null() {
        let _ = unsafe { GlobalFree(Some(dev_mode)) };
        let _ = unsafe { ClosePrinter(printer) };
        return HGLOBAL::default();
    }

    let dev_mode_ptr = raw_dev_mode.cast::<DEVMODEW>();
    let initialized = unsafe {
        DocumentPropertiesW(
            None,
            printer,
            printer_name,
            Some(dev_mode_ptr),
            None,
            DM_OUT_BUFFER.0,
        )
    };
    if initialized != DOCUMENT_PROPERTIES_OK {
        let _ = unsafe { GlobalUnlock(dev_mode) };
        let _ = unsafe { GlobalFree(Some(dev_mode)) };
        let _ = unsafe { ClosePrinter(printer) };
        return HGLOBAL::default();
    }

    let orientation = if page.width >= page.height {
        DMORIENT_LANDSCAPE as i16
    } else {
        DMORIENT_PORTRAIT as i16
    };
    unsafe {
        let dev_mode = &mut *dev_mode_ptr;
        dev_mode.dmFields = DEVMODE_FIELD_FLAGS(dev_mode.dmFields.0 | DM_ORIENTATION.0);
        dev_mode.Anonymous1.Anonymous1.dmOrientation = orientation;
    }

    let merged = unsafe {
        DocumentPropertiesW(
            None,
            printer,
            printer_name,
            Some(dev_mode_ptr),
            Some(dev_mode_ptr.cast_const()),
            DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
        )
    };

    let _ = unsafe { GlobalUnlock(dev_mode) };
    let _ = unsafe { ClosePrinter(printer) };

    if merged != DOCUMENT_PROPERTIES_OK {
        let _ = unsafe { GlobalFree(Some(dev_mode)) };
        return HGLOBAL::default();
    }

    dev_mode
}

#[cfg(windows)]
fn print_to_hdc(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    page: &RenderedPage,
) -> Result<(), String> {
    use std::mem::size_of;

    use windows::Win32::Graphics::Gdi::{SetBkMode, TRANSPARENT};
    use windows::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
    use windows::core::PCWSTR;

    let doc_name = wide_null("ECG Studio");
    let doc_info = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(doc_name.as_ptr()),
        ..Default::default()
    };

    if unsafe { StartDocW(hdc, &doc_info) } <= 0 {
        return Err("Falha ao iniciar o trabalho de impressão.".to_owned());
    }

    if unsafe { StartPage(hdc) } <= 0 {
        let _ = unsafe { EndDoc(hdc) };
        return Err("Falha ao iniciar a página de impressão.".to_owned());
    }

    let target = print_target_rect(hdc);
    let scale = (target.width / page.width).min(target.height / page.height);
    let transform = DeviceTransform {
        offset_x: target.left + ((target.width - (page.width * scale)) / 2.0),
        offset_y: target.top + ((target.height - (page.height * scale)) / 2.0),
        scale,
    };

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
    }

    for command in &page.commands {
        draw_command(hdc, transform, command);
    }

    let end_page = unsafe { EndPage(hdc) };
    let end_doc = unsafe { EndDoc(hdc) };
    if end_page <= 0 || end_doc <= 0 {
        return Err("Falha ao finalizar a impressão.".to_owned());
    }

    Ok(())
}

#[cfg(windows)]
fn print_target_rect(hdc: windows::Win32::Graphics::Gdi::HDC) -> PrintTargetRect {
    use windows::Win32::Graphics::Gdi::{
        GetDeviceCaps, HORZRES, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH,
        VERTRES,
    };

    let printable_width = unsafe { GetDeviceCaps(Some(hdc), HORZRES) }.max(1);
    let printable_height = unsafe { GetDeviceCaps(Some(hdc), VERTRES) }.max(1);
    let physical_width = unsafe { GetDeviceCaps(Some(hdc), PHYSICALWIDTH) };
    let physical_height = unsafe { GetDeviceCaps(Some(hdc), PHYSICALHEIGHT) };
    if physical_width > 0 && physical_height > 0 {
        return PrintTargetRect {
            left: -(unsafe { GetDeviceCaps(Some(hdc), PHYSICALOFFSETX) } as f64),
            top: -(unsafe { GetDeviceCaps(Some(hdc), PHYSICALOFFSETY) } as f64),
            width: physical_width as f64,
            height: physical_height as f64,
        };
    }

    PrintTargetRect {
        left: 0.0,
        top: 0.0,
        width: printable_width as f64,
        height: printable_height as f64,
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct PrintTargetRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct DeviceTransform {
    offset_x: f64,
    offset_y: f64,
    scale: f64,
}

#[cfg(windows)]
impl DeviceTransform {
    fn point(self, point: Point) -> windows::Win32::Foundation::POINT {
        windows::Win32::Foundation::POINT {
            x: (self.offset_x + point.x * self.scale).round() as i32,
            y: (self.offset_y + point.y * self.scale).round() as i32,
        }
    }

    fn rect(self, rect: Rect) -> windows::Win32::Foundation::RECT {
        let left = self.offset_x + rect.left * self.scale;
        let top = self.offset_y + rect.top * self.scale;
        let right = self.offset_x + rect.right * self.scale;
        let bottom = self.offset_y + rect.bottom * self.scale;
        windows::Win32::Foundation::RECT {
            left: left.round() as i32,
            top: top.round() as i32,
            right: right.round() as i32,
            bottom: bottom.round() as i32,
        }
    }

    fn stroke(self, width: f64) -> i32 {
        (width * self.scale).round().max(1.0) as i32
    }

    fn font_height(self, size: f64) -> i32 {
        -((size * self.scale).round().max(1.0) as i32)
    }
}

#[cfg(windows)]
fn draw_command(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    transform: DeviceTransform,
    command: &DrawCommand,
) {
    match command {
        DrawCommand::FillRect { rect, color } => fill_rect(hdc, transform.rect(*rect), *color),
        DrawCommand::StrokeRect {
            rect,
            color,
            stroke_width,
        } => stroke_rect(
            hdc,
            transform.rect(*rect),
            *color,
            transform.stroke(*stroke_width),
        ),
        DrawCommand::Line {
            start,
            end,
            color,
            stroke_width,
        } => draw_line(
            hdc,
            transform.point(*start),
            transform.point(*end),
            *color,
            transform.stroke(*stroke_width),
        ),
        DrawCommand::Polyline {
            points,
            color,
            stroke_width,
        } => draw_polyline(
            hdc,
            points.iter().map(|point| transform.point(*point)).collect(),
            *color,
            transform.stroke(*stroke_width),
        ),
        DrawCommand::Image { rect, bitmap, .. } => {
            if let Some(bitmap) = bitmap {
                draw_bitmap(hdc, transform.rect(*rect), bitmap);
            }
        }
        DrawCommand::Text {
            x,
            y,
            text,
            font_size,
            color,
            weight,
        } => draw_text(
            hdc,
            transform.point(Point { x: *x, y: *y }),
            text,
            transform.font_height(*font_size),
            *color,
            *weight as i32,
        ),
    }
}

#[cfg(windows)]
fn draw_bitmap(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    bitmap: &LogoBitmap,
) {
    use std::ffi::c_void;
    use std::mem::size_of;

    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY, StretchDIBits,
    };

    if bitmap.width == 0 || bitmap.height == 0 || bitmap.rgba.is_empty() {
        return;
    }

    let dest_width = (rect.right - rect.left).max(1);
    let dest_height = (rect.bottom - rect.top).max(1);
    let pixels = rgba_to_bgra(&bitmap.rgba);
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bitmap.width as i32,
            biHeight: -(bitmap.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: pixels.len() as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    unsafe {
        let _ = StretchDIBits(
            hdc,
            rect.left,
            rect.top,
            dest_width,
            dest_height,
            0,
            0,
            bitmap.width as i32,
            bitmap.height as i32,
            Some(pixels.as_ptr() as *const c_void),
            &bitmap_info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

#[cfg(windows)]
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    bgra
}

#[cfg(windows)]
fn fill_rect(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    color: Color,
) {
    use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect};

    let brush = unsafe { CreateSolidBrush(colorref(color)) };
    if brush.0.is_null() {
        return;
    }

    unsafe {
        let _ = FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

#[cfg(windows)]
fn stroke_rect(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: windows::Win32::Foundation::RECT,
    color: Color,
    stroke_width: i32,
) {
    draw_line(
        hdc,
        windows::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.top,
        },
        windows::Win32::Foundation::POINT {
            x: rect.right,
            y: rect.top,
        },
        color,
        stroke_width,
    );
    draw_line(
        hdc,
        windows::Win32::Foundation::POINT {
            x: rect.right,
            y: rect.top,
        },
        windows::Win32::Foundation::POINT {
            x: rect.right,
            y: rect.bottom,
        },
        color,
        stroke_width,
    );
    draw_line(
        hdc,
        windows::Win32::Foundation::POINT {
            x: rect.right,
            y: rect.bottom,
        },
        windows::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.bottom,
        },
        color,
        stroke_width,
    );
    draw_line(
        hdc,
        windows::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.bottom,
        },
        windows::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.top,
        },
        color,
        stroke_width,
    );
}

#[cfg(windows)]
fn draw_line(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    start: windows::Win32::Foundation::POINT,
    end: windows::Win32::Foundation::POINT,
    color: Color,
    stroke_width: i32,
) {
    use windows::Win32::Graphics::Gdi::{
        CreatePen, DeleteObject, LineTo, MoveToEx, PS_SOLID, SelectObject,
    };

    let pen = unsafe { CreatePen(PS_SOLID, stroke_width, colorref(color)) };
    if pen.0.is_null() {
        return;
    }

    unsafe {
        let old_pen = SelectObject(hdc, pen.into());
        let _ = MoveToEx(hdc, start.x, start.y, None);
        let _ = LineTo(hdc, end.x, end.y);
        if !old_pen.0.is_null() {
            let _ = SelectObject(hdc, old_pen);
        }
        let _ = DeleteObject(pen.into());
    }
}

#[cfg(windows)]
fn draw_polyline(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    points: Vec<windows::Win32::Foundation::POINT>,
    color: Color,
    stroke_width: i32,
) {
    use windows::Win32::Graphics::Gdi::{
        CreatePen, DeleteObject, PS_SOLID, Polyline, SelectObject,
    };

    if points.len() < 2 {
        return;
    }

    let pen = unsafe { CreatePen(PS_SOLID, stroke_width, colorref(color)) };
    if pen.0.is_null() {
        return;
    }

    unsafe {
        let old_pen = SelectObject(hdc, pen.into());
        let mut start = 0;
        while start + 1 < points.len() {
            let end = (start + 16_384).min(points.len());
            let _ = Polyline(hdc, &points[start..end]);
            if end == points.len() {
                break;
            }
            start = end - 1;
        }
        if !old_pen.0.is_null() {
            let _ = SelectObject(hdc, old_pen);
        }
        let _ = DeleteObject(pen.into());
    }
}

#[cfg(windows)]
fn draw_text(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    origin: windows::Win32::Foundation::POINT,
    text: &str,
    font_height: i32,
    color: Color,
    weight: i32,
) {
    use windows::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DeleteObject,
        FF_SWISS, OUT_DEFAULT_PRECIS, SelectObject, SetTextColor, TextOutW,
    };
    use windows::core::PCWSTR;

    if text.is_empty() {
        return;
    }

    let face_name = wide_null("Segoe UI Variable Text");
    let font = unsafe {
        CreateFontW(
            font_height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_SWISS.0 as u32,
            PCWSTR(face_name.as_ptr()),
        )
    };
    if font.0.is_null() {
        return;
    }

    let wide_text: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let old_font = SelectObject(hdc, font.into());
        let _ = SetTextColor(hdc, colorref(color));
        let _ = TextOutW(hdc, origin.x, origin.y, &wide_text);
        if !old_font.0.is_null() {
            let _ = SelectObject(hdc, old_font);
        }
        let _ = DeleteObject(font.into());
    }
}

#[cfg(windows)]
fn colorref(color: Color) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(
        color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16),
    )
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn free_print_dialog_handles(
    dev_mode: windows::Win32::Foundation::HGLOBAL,
    dev_names: windows::Win32::Foundation::HGLOBAL,
) {
    use windows::Win32::Foundation::GlobalFree;

    unsafe {
        if !dev_mode.0.is_null() {
            let _ = GlobalFree(Some(dev_mode));
        }
        if !dev_names.0.is_null() {
            let _ = GlobalFree(Some(dev_names));
        }
    }
}

#[cfg(target_os = "linux")]
fn print_page_platform(page: &RenderedPage) -> Result<(), String> {
    if page.width <= 0.0 || page.height <= 0.0 || page.commands.is_empty() {
        return Err("Não há ECG vetorial para imprimir.".to_owned());
    }

    let pdf = page_to_pdf(page)?;
    let path = temporary_print_pdf_path();
    std::fs::write(&path, &pdf)
        .map_err(|error| format!("Falha preparando a página de impressão: {error}"))?;

    match print_pdf_with_dialog(&path, page) {
        Ok(()) => Ok(()),
        Err(error) if is_print_cancelled(&error) => Err("Impressão cancelada.".to_owned()),
        Err(dialog_error) => print_via_html_fallback(page).map_err(|fallback_error| {
            format!("Falha ao abrir o diálogo de impressão: {dialog_error}. {fallback_error}")
        }),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn print_page_platform(page: &RenderedPage) -> Result<(), String> {
    print_via_html_fallback(page)
}

#[cfg(not(windows))]
fn print_via_html_fallback(page: &RenderedPage) -> Result<(), String> {
    if page.width <= 0.0 || page.height <= 0.0 || page.commands.is_empty() {
        return Err("Não há ECG vetorial para imprimir.".to_owned());
    }

    let path = temporary_print_html_path();
    std::fs::write(&path, print_dialog_html(page))
        .map_err(|error| format!("Falha preparando a página de impressão: {error}"))?;

    open_print_dialog_html(&path).map_err(|error| {
        format!(
            "Falha ao abrir o diálogo de impressão: {error}. Arquivo preparado em {}.",
            path.display()
        )
    })
}

#[cfg(target_os = "linux")]
fn print_pdf_with_dialog(path: &std::path::Path, page: &RenderedPage) -> Result<(), String> {
    use std::fs::File;
    use std::os::fd::AsFd;

    use ashpd::desktop::print::{Orientation, PageSetup, PrintProxy, Settings};

    let file = File::open(path).map_err(|error| {
        format!(
            "Falha lendo a página de impressão {}: {error}",
            path.display()
        )
    })?;
    let orientation = if page.width >= page.height {
        Orientation::Landscape
    } else {
        Orientation::Portrait
    };
    let settings = Settings::default()
        .orientation(orientation)
        .paper_format("iso_a4_210x297mm")
        .n_copies("1");
    let page_setup = if page.width >= page.height {
        PageSetup::default()
            .orientation(orientation)
            .width(297.0)
            .height(210.0)
    } else {
        PageSetup::default()
            .orientation(orientation)
            .width(210.0)
            .height(297.0)
    };

    pollster::block_on(async {
        let proxy = PrintProxy::new()
            .await
            .map_err(|error| format!("portal de impressão indisponível ({error})"))?;
        let prepared = proxy
            .prepare_print(None, "ECG Studio", settings, page_setup, None, true)
            .await
            .map_err(format_portal_error)?
            .response()
            .map_err(format_portal_error)?;
        proxy
            .print(
                None,
                "ECG Studio",
                &file.as_fd(),
                Some(prepared.token),
                true,
            )
            .await
            .map_err(format_portal_error)?;
        Ok(())
    })
}

#[cfg(target_os = "linux")]
fn format_portal_error(error: ashpd::Error) -> String {
    let text = error.to_string();
    if is_print_cancelled(&text) {
        "Impressão cancelada.".to_owned()
    } else {
        text
    }
}

#[cfg(target_os = "linux")]
fn is_print_cancelled(error: &str) -> bool {
    error.to_ascii_lowercase().contains("cancel")
}

#[cfg(target_os = "linux")]
fn temporary_print_pdf_path() -> std::path::PathBuf {
    temporary_print_path("pdf")
}

#[cfg(any(target_os = "linux", test))]
fn page_to_pdf(page: &RenderedPage) -> Result<Vec<u8>, String> {
    use std::io::Write;

    let landscape = page.width >= page.height;
    let (media_w, media_h) = if landscape {
        (842.0_f64, 595.0_f64)
    } else {
        (595.0_f64, 842.0_f64)
    };
    if page.width <= 0.0 || page.height <= 0.0 {
        return Err("Página de ECG inválida para PDF.".to_owned());
    }

    let mut images = Vec::new();
    let mut content = String::new();
    content.push_str("q\n");
    let _ = write!(
        content,
        "{:.6} 0 0 {:.6} 0 {:.6} cm\n",
        media_w / page.width,
        -(media_h / page.height),
        media_h
    );
    content.push_str("1 J 1 j\n");

    for command in &page.commands {
        match command {
            DrawCommand::FillRect { rect, color } => {
                pdf_set_fill_color(&mut content, *color);
                let _ = write!(
                    content,
                    "{:.3} {:.3} {:.3} {:.3} re f\n",
                    rect.left,
                    rect.top,
                    rect.width(),
                    rect.height()
                );
            }
            DrawCommand::StrokeRect {
                rect,
                color,
                stroke_width,
            } => {
                pdf_set_stroke(&mut content, *color, *stroke_width);
                let _ = write!(
                    content,
                    "{:.3} {:.3} {:.3} {:.3} re S\n",
                    rect.left,
                    rect.top,
                    rect.width(),
                    rect.height()
                );
            }
            DrawCommand::Line {
                start,
                end,
                color,
                stroke_width,
            } => {
                pdf_set_stroke(&mut content, *color, *stroke_width);
                let _ = write!(
                    content,
                    "{:.3} {:.3} m {:.3} {:.3} l S\n",
                    start.x, start.y, end.x, end.y
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
                pdf_set_stroke(&mut content, *color, *stroke_width);
                pdf_write_polyline(&mut content, points);
            }
            DrawCommand::Image { rect, bitmap, .. } => {
                let Some(bitmap) = bitmap else {
                    continue;
                };
                let Some(jpeg) = jpeg_from_rgba(bitmap) else {
                    continue;
                };
                let name = format!("Im{}", images.len());
                let _ = write!(
                    content,
                    "q {:.3} 0 0 {:.3} {:.3} {:.3} cm /{name} Do Q\n",
                    rect.width(),
                    -rect.height(),
                    rect.left,
                    rect.top + rect.height()
                );
                images.push((name, bitmap.width, bitmap.height, jpeg));
            }
            DrawCommand::Text {
                x,
                y,
                text,
                font_size,
                color,
                weight,
            } => pdf_write_text(&mut content, *x, *y, text, *font_size, *color, *weight),
        }
    }
    content.push_str("Q\n");

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::new();

    let write_obj = |pdf: &mut Vec<u8>, offsets: &mut Vec<u32>, body: &[u8]| {
        offsets.push(pdf.len() as u32);
        let id = offsets.len();
        let _ = write!(pdf, "{id} 0 obj\n");
        pdf.extend_from_slice(body);
        if !body.ends_with(b"\n") {
            pdf.push(b'\n');
        }
        pdf.extend_from_slice(b"endobj\n");
    };

    write_obj(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Catalog /Pages 2 0 R >>\n",
    );
    write_obj(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n",
    );

    let mut xobjects = String::new();
    for (index, (name, _, _, _)) in images.iter().enumerate() {
        let id = 7 + index;
        let _ = write!(xobjects, "/{name} {id} 0 R ");
    }
    let resources =
        format!("/Resources << /Font << /F1 5 0 R /F2 6 0 R >> /XObject << {xobjects}>> >>");
    let page_obj = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_w:.2} {media_h:.2}] /Contents 4 0 R {resources} >>\n"
    );
    write_obj(&mut pdf, &mut offsets, page_obj.as_bytes());

    let content_bytes = content.into_bytes();
    let mut contents_obj = Vec::new();
    let _ = write!(
        contents_obj,
        "<< /Length {} >>\nstream\n",
        content_bytes.len()
    );
    contents_obj.extend_from_slice(&content_bytes);
    contents_obj.extend_from_slice(b"endstream\n");
    write_obj(&mut pdf, &mut offsets, &contents_obj);

    write_obj(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\n",
    );
    write_obj(
        &mut pdf,
        &mut offsets,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>\n",
    );

    for (name, width, height, jpeg) in &images {
        let _ = name;
        let mut image_obj = Vec::new();
        let _ = write!(
            image_obj,
            "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        );
        image_obj.extend_from_slice(jpeg);
        image_obj.extend_from_slice(b"\nendstream\n");
        write_obj(&mut pdf, &mut offsets, &image_obj);
    }

    let xref_offset = pdf.len();
    let size = offsets.len() + 1;
    let _ = write!(pdf, "xref\n0 {size}\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        let _ = write!(pdf, "{offset:010} 00000 n \n");
    }
    let _ = write!(
        pdf,
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    );

    Ok(pdf)
}

#[cfg(any(target_os = "linux", test))]
fn pdf_set_fill_color(content: &mut String, color: Color) {
    let _ = write!(
        content,
        "{:.3} {:.3} {:.3} rg\n",
        color.r as f64 / 255.0,
        color.g as f64 / 255.0,
        color.b as f64 / 255.0
    );
}

#[cfg(any(target_os = "linux", test))]
fn pdf_set_stroke(content: &mut String, color: Color, stroke_width: f64) {
    let _ = write!(
        content,
        "{:.3} {:.3} {:.3} RG {:.3} w\n",
        color.r as f64 / 255.0,
        color.g as f64 / 255.0,
        color.b as f64 / 255.0,
        stroke_width.max(0.2)
    );
}

#[cfg(any(target_os = "linux", test))]
fn pdf_write_polyline(content: &mut String, points: &[Point]) {
    let mut start = 0;
    while start + 1 < points.len() {
        let end = (start + 16_384).min(points.len());
        let _ = write!(content, "{:.3} {:.3} m", points[start].x, points[start].y);
        for point in &points[start + 1..end] {
            let _ = write!(content, " {:.3} {:.3} l", point.x, point.y);
        }
        content.push_str(" S\n");
        if end == points.len() {
            break;
        }
        start = end - 1;
    }
}

#[cfg(any(target_os = "linux", test))]
fn pdf_write_text(
    content: &mut String,
    x: f64,
    y: f64,
    text: &str,
    font_size: f64,
    color: Color,
    weight: u16,
) {
    let encoded = pdf_encode_winansi(text);
    if encoded.is_empty() {
        return;
    }
    let font = if weight >= 600 { "F2" } else { "F1" };
    let size = font_size.max(1.0);
    pdf_set_fill_color(content, color);
    let _ = write!(
        content,
        "BT /{font} {size:.3} Tf 1 0 0 -1 {x:.3} {baseline:.3} Tm (",
        baseline = y + size * 0.8
    );
    content.push_str(&encoded);
    content.push_str(") Tj ET\n");
}

#[cfg(any(target_os = "linux", test))]
fn pdf_encode_winansi(text: &str) -> String {
    let mut encoded = String::new();
    for ch in text.chars() {
        let Some(byte) = winansi_byte(ch) else {
            encoded.push('?');
            continue;
        };
        match byte {
            b'\\' => encoded.push_str("\\\\"),
            b'(' => encoded.push_str("\\("),
            b')' => encoded.push_str("\\)"),
            32..=126 => encoded.push(byte as char),
            _ => {
                let _ = write!(encoded, "\\{byte:03o}");
            }
        }
    }
    encoded
}

#[cfg(any(target_os = "linux", test))]
fn winansi_byte(ch: char) -> Option<u8> {
    let value = ch as u32;
    if value <= 127 || (0xA0..=0xFF).contains(&value) {
        Some(value as u8)
    } else {
        None
    }
}

#[cfg(any(target_os = "linux", test))]
fn jpeg_from_rgba(bitmap: &LogoBitmap) -> Option<Vec<u8>> {
    use image::ExtendedColorType;
    use image::codecs::jpeg::JpegEncoder;

    if bitmap.width == 0 || bitmap.height == 0 || bitmap.rgba.len() < 4 {
        return None;
    }

    let mut rgb = Vec::with_capacity(bitmap.width as usize * bitmap.height as usize * 3);
    for pixel in bitmap.rgba.chunks_exact(4) {
        let alpha = pixel[3] as u16;
        let blend = |channel: u8| ((channel as u16 * alpha + 255 * (255 - alpha)) / 255) as u8;
        rgb.push(blend(pixel[0]));
        rgb.push(blend(pixel[1]));
        rgb.push(blend(pixel[2]));
    }

    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 90);
    encoder
        .encode(&rgb, bitmap.width, bitmap.height, ExtendedColorType::Rgb8)
        .ok()?;
    Some(jpeg)
}

#[cfg(not(windows))]
fn temporary_print_path(extension: &str) -> std::path::PathBuf {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("ecg-studio-print-{millis}.{extension}"))
}

#[cfg(not(windows))]
fn print_dialog_html(page: &RenderedPage) -> String {
    let orientation = if page.width >= page.height {
        "landscape"
    } else {
        "portrait"
    };
    let svg = page.to_svg();

    format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>ECG Studio</title>
<style>
@page {{ size: A4 {orientation}; margin: 0; }}
html, body {{ margin: 0; width: 100%; height: 100%; background: white; }}
body {{ display: flex; align-items: center; justify-content: center; }}
svg {{ width: 100vw; height: 100vh; }}
</style>
</head>
<body>
{svg}
<script>
window.addEventListener("load", function () {{
    window.focus();
    window.print();
}});
</script>
</body>
</html>
"#
    )
}

#[cfg(not(windows))]
fn temporary_print_html_path() -> std::path::PathBuf {
    temporary_print_path("html")
}

#[cfg(not(windows))]
fn open_print_dialog_html(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("comando open indisponivel ({error})"))
    }

    #[cfg(target_os = "linux")]
    {
        let first = std::process::Command::new("xdg-open").arg(path).spawn();
        match first {
            Ok(_) => Ok(()),
            Err(first_error) => std::process::Command::new("gio")
                .arg("open")
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|second_error| {
                    format!("xdg-open falhou ({first_error}); gio open falhou ({second_error})")
                }),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = path;
        Err("sistema operacional nao possui fluxo de impressao configurado".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{jpeg_from_rgba, page_to_pdf, pdf_encode_winansi};
    use crate::preview::{Color, DrawCommand, LogoBitmap, Point, Rect, RenderedPage};

    fn sample_page(width: f64, height: f64) -> RenderedPage {
        RenderedPage {
            width,
            height,
            commands: vec![
                DrawCommand::FillRect {
                    rect: Rect {
                        left: 0.0,
                        top: 0.0,
                        right: width,
                        bottom: height,
                    },
                    color: Color {
                        r: 255,
                        g: 255,
                        b: 255,
                    },
                },
                DrawCommand::Line {
                    start: Point { x: 10.0, y: 10.0 },
                    end: Point { x: 120.0, y: 80.0 },
                    color: Color { r: 0, g: 0, b: 0 },
                    stroke_width: 1.2,
                },
                DrawCommand::Polyline {
                    points: vec![
                        Point { x: 20.0, y: 40.0 },
                        Point { x: 40.0, y: 60.0 },
                        Point { x: 80.0, y: 30.0 },
                    ],
                    color: Color { r: 0, g: 0, b: 0 },
                    stroke_width: 0.8,
                },
                DrawCommand::Text {
                    x: 24.0,
                    y: 18.0,
                    text: "ECG (I)".to_owned(),
                    font_size: 12.0,
                    color: Color {
                        r: 17,
                        g: 17,
                        b: 17,
                    },
                    weight: 700,
                },
            ],
        }
    }

    fn pdf_as_text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn writes_landscape_a4_pdf() {
        let pdf = page_to_pdf(&sample_page(1600.0, 1131.0)).expect("pdf");
        let text = pdf_as_text(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/MediaBox [0 0 842.00 595.00]"));
        assert!(text.contains("/BaseFont /Helvetica"));
        assert!(text.contains("ECG \\(I\\)"));
        assert!(text.contains("%%EOF"));
    }

    #[test]
    fn writes_portrait_a4_pdf() {
        let pdf = page_to_pdf(&sample_page(1131.0, 1600.0)).expect("pdf");
        let text = pdf_as_text(&pdf);
        assert!(text.contains("/MediaBox [0 0 595.00 842.00]"));
    }

    #[test]
    fn embeds_logo_jpeg() {
        let mut page = sample_page(1600.0, 1131.0);
        page.commands.push(DrawCommand::Image {
            rect: Rect {
                left: 10.0,
                top: 10.0,
                right: 80.0,
                bottom: 50.0,
            },
            data_uri: String::new(),
            bitmap: Some(LogoBitmap {
                width: 2,
                height: 2,
                rgba: vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            }),
        });
        let pdf = page_to_pdf(&page).expect("pdf");
        let text = pdf_as_text(&pdf);
        assert!(text.contains("/Subtype /Image"));
        assert!(text.contains("/Im0 "));
        assert!(
            jpeg_from_rgba(
                page.commands
                    .iter()
                    .find_map(|command| match command {
                        DrawCommand::Image { bitmap, .. } => bitmap.as_ref(),
                        _ => None,
                    })
                    .expect("bitmap")
            )
            .is_some()
        );
        assert!(pdf.windows(2).any(|window| window == b"\xff\xd8"));
    }

    #[test]
    fn encodes_winansi_accents_and_parentheses() {
        assert_eq!(pdf_encode_winansi("ECG (I)"), "ECG \\(I\\)");
        assert_eq!(pdf_encode_winansi("São"), "S\\343o");
    }
}
