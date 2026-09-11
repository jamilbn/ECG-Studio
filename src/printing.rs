use crate::preview::RenderedPage;
#[cfg(windows)]
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

#[cfg(not(windows))]
fn print_page_platform(page: &RenderedPage) -> Result<(), String> {
    if page.width <= 0.0 || page.height <= 0.0 || page.commands.is_empty() {
        return Err("Nao ha ECG vetorial para imprimir.".to_owned());
    }

    let path = temporary_print_html_path();
    std::fs::write(&path, print_dialog_html(page))
        .map_err(|error| format!("Falha preparando a pagina de impressao: {error}"))?;

    open_print_dialog_html(&path).map_err(|error| {
        format!(
            "Falha ao abrir o dialogo de impressao: {error}. Arquivo preparado em {}.",
            path.display()
        )
    })
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
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("ecg-studio-print-{millis}.html"))
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
