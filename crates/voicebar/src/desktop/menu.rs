//! Native HMENU behavior with VoiceBar's shared visual language.
//! Rows keep their strings for accessibility; itemData is a checked row index,
//! never a pointer into a Vec that Windows could outlive.
use crate::palette;
use std::mem::size_of;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, RECT, SIZE},
        Graphics::Gdi::*,
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Controls::{
                DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT,
                ODS_SELECTED, ODT_MENU,
            },
            WindowsAndMessaging::*,
        },
    },
};

pub(super) struct Item {
    pub action: usize,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
}

struct Row {
    action: u32,
    label: Vec<u16>,
    hint: Vec<u16>,
    enabled: bool,
    separator: bool,
}

struct GdiObject(HGDIOBJ);
impl GdiObject {
    fn new(object: HGDIOBJ) -> Result<Self, String> {
        if object.is_invalid() {
            Err("无法创建托盘菜单绘制资源".into())
        } else {
            Ok(Self(object))
        }
    }
    fn brush(color: u32) -> Result<Self, String> {
        Self::new(HGDIOBJ(unsafe { CreateSolidBrush(colorref(color)) }.0))
    }
    fn as_brush(&self) -> HBRUSH {
        HBRUSH(self.0 .0)
    }
}
impl Drop for GdiObject {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

struct WindowDc(HWND, HDC);
impl Drop for WindowDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(Some(self.0), self.1);
        }
    }
}
struct DcState(HDC, i32);
impl DcState {
    unsafe fn save(dc: HDC) -> Option<Self> {
        let saved = SaveDC(dc);
        (saved != 0).then_some(Self(dc, saved))
    }
}
impl Drop for DcState {
    fn drop(&mut self) {
        unsafe {
            let _ = RestoreDC(self.0, self.1);
        }
    }
}

struct Theme {
    font: GdiObject,
    background: GdiObject,
    selection: GdiObject,
    line: GdiObject,
    dpi: u32,
    width: u32,
    row_height: u32,
}
impl Theme {
    fn new(owner: HWND, dpi: u32, rows: &[Row]) -> Result<Self, String> {
        let mut theme = Self {
            font: GdiObject::new(HGDIOBJ(
                unsafe {
                    CreateFontW(
                        -scaled(13, dpi),
                        0,
                        0,
                        0,
                        400,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        CLEARTYPE_QUALITY,
                        DEFAULT_PITCH.0 as u32,
                        PCWSTR(wide(palette::FONT_FAMILY).as_ptr()),
                    )
                }
                .0,
            ))?,
            background: GdiObject::brush(palette::PANEL)?,
            selection: GdiObject::brush(palette::TINT)?,
            line: GdiObject::brush(palette::LINE)?,
            dpi,
            width: scaled(220, dpi) as u32,
            row_height: scaled(40, dpi) as u32,
        };
        unsafe {
            let dc = WindowDc(owner, GetDC(Some(owner)));
            if dc.1.is_invalid() {
                return Err("无法测量托盘菜单文字".into());
            }
            let _state = DcState::save(dc.1).ok_or("无法保存菜单绘制状态")?;
            SelectObject(dc.1, theme.font.0);
            let (mut label_width, mut hint_width, mut text_height) = (0, 0, 0);
            for row in rows.iter().filter(|row| !row.separator) {
                for (text, width) in [(&row.label, &mut label_width), (&row.hint, &mut hint_width)]
                {
                    if text.is_empty() {
                        continue;
                    }
                    let mut size = SIZE::default();
                    if !GetTextExtentPoint32W(dc.1, text, &mut size).as_bool() {
                        return Err("无法测量托盘菜单文字".into());
                    }
                    *width = (*width).max(size.cx);
                    text_height = text_height.max(size.cy);
                }
            }
            let gap = if hint_width > 0 { scaled(32, dpi) } else { 0 };
            theme.width = theme
                .width
                .max((label_width + hint_width + gap + scaled(32, dpi)) as u32);
            theme.row_height = theme.row_height.max((text_height + scaled(16, dpi)) as u32);
        }
        Ok(theme)
    }

    unsafe fn draw(&self, row: &Row, item: &DRAWITEMSTRUCT) {
        let Some(_state) = DcState::save(item.hDC) else {
            return;
        };
        let rect = item.rcItem;
        FillRect(item.hDC, &rect, self.background.as_brush());
        if row.separator {
            let line = RECT {
                left: rect.left + scaled(12, self.dpi),
                right: rect.right - scaled(12, self.dpi),
                top: (rect.top + rect.bottom) / 2,
                bottom: (rect.top + rect.bottom) / 2 + scaled(1, self.dpi),
            };
            FillRect(item.hDC, &line, self.line.as_brush());
            return;
        }
        let disabled = !row.enabled || item.itemState.0 & (ODS_DISABLED.0 | ODS_GRAYED.0) != 0;
        let selected = !disabled && item.itemState.0 & (ODS_SELECTED.0 | ODS_HOTLIGHT.0) != 0;
        if selected {
            SelectObject(item.hDC, self.selection.0);
            SelectObject(item.hDC, GetStockObject(NULL_PEN));
            let radius = scaled(6, self.dpi);
            let _ = RoundRect(
                item.hDC,
                rect.left + scaled(4, self.dpi),
                rect.top + scaled(2, self.dpi),
                rect.right - scaled(4, self.dpi),
                rect.bottom - scaled(2, self.dpi),
                radius * 2,
                radius * 2,
            );
        }
        SelectObject(item.hDC, self.font.0);
        SetBkMode(item.hDC, TRANSPARENT);
        let text_color = if disabled {
            palette::MUTED
        } else if selected {
            palette::MINT
        } else {
            palette::TEXT
        };
        let mut text_rect = RECT {
            left: rect.left + scaled(16, self.dpi),
            right: rect.right - scaled(16, self.dpi),
            ..rect
        };
        let flags = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
        SetTextColor(item.hDC, colorref(text_color));
        DrawTextW(
            item.hDC,
            &mut row.label.clone(),
            &mut text_rect,
            flags | DT_LEFT,
        );
        if !row.hint.is_empty() {
            SetTextColor(
                item.hDC,
                colorref(if selected {
                    palette::MINT
                } else {
                    palette::MUTED
                }),
            );
            DrawTextW(
                item.hDC,
                &mut row.hint.clone(),
                &mut text_rect,
                flags | DT_RIGHT,
            );
        }
    }
}

pub(super) struct Popup {
    handle: HMENU,
    rows: Vec<Row>,
    // DestroyMenu must run before releasing the background brush Windows holds.
    theme: Option<Theme>,
}
impl Popup {
    pub(super) fn new(owner: isize, dpi: u32, items: Vec<Item>) -> Result<Self, String> {
        Self::create(owner, dpi, items, high_contrast())
    }

    fn create(
        owner: isize,
        dpi: u32,
        items: Vec<Item>,
        system_colors: bool,
    ) -> Result<Self, String> {
        let rows = items
            .iter()
            .map(|item| {
                let (label, hint) = item.label.split_once('\t').unwrap_or((&item.label, ""));
                Row {
                    action: item.action as u32,
                    label: label.encode_utf16().collect(),
                    hint: hint.encode_utf16().collect(),
                    enabled: item.enabled,
                    separator: item.separator,
                }
            })
            .collect::<Vec<_>>();
        let theme = if system_colors {
            None
        } else {
            Some(Theme::new(HWND(owner as *mut _), dpi.max(96), &rows)?)
        };
        let handle = unsafe { CreatePopupMenu() }.map_err(|e| format!("无法创建托盘菜单：{e}"))?;
        let popup = Self {
            handle,
            rows,
            theme,
        };
        unsafe {
            if let Some(theme) = &popup.theme {
                let info = MENUINFO {
                    cbSize: size_of::<MENUINFO>() as u32,
                    fMask: MIM_BACKGROUND | MIM_STYLE,
                    hbrBack: theme.background.as_brush(),
                    dwStyle: MNS_NOCHECK,
                    ..Default::default()
                };
                SetMenuInfo(handle, &info).map_err(|e| format!("无法设置托盘菜单主题：{e}"))?;
            }
            for (index, item) in items.iter().enumerate() {
                let mut label = wide(&item.label);
                let mut info = MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_ID | MIIM_STATE | MIIM_FTYPE,
                    wID: item.action as u32,
                    fState: if item.enabled {
                        MFS_ENABLED
                    } else {
                        MFS_DISABLED
                    },
                    fType: if item.separator {
                        MFT_SEPARATOR
                    } else {
                        MFT_STRING
                    },
                    ..Default::default()
                };
                if !item.separator {
                    info.fMask |= MIIM_STRING;
                    info.dwTypeData = PWSTR(label.as_mut_ptr());
                    info.cch = (label.len() - 1) as u32;
                }
                if popup.theme.is_some() {
                    info.fMask |= MIIM_DATA;
                    info.fType |= MFT_OWNERDRAW;
                    info.dwItemData = index + 1;
                }
                InsertMenuItemW(handle, index as u32, true, &info)
                    .map_err(|e| format!("无法添加托盘菜单项：{e}"))?;
            }
        }
        Ok(popup)
    }

    pub(super) fn handle(&self) -> isize {
        self.handle.0 as isize
    }

    // Called only while this Popup is alive in TrackPopupMenu. No runtime locks
    // or RefCell borrows cross the nested Windows message loop.
    pub(super) unsafe fn message(
        &self,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> Option<isize> {
        let theme = self.theme.as_ref()?;
        if wparam != 0 || lparam == 0 {
            return None;
        }
        match message {
            WM_MEASUREITEM => {
                let item = &mut *(lparam as *mut MEASUREITEMSTRUCT);
                if item.CtlType != ODT_MENU {
                    return None;
                }
                let row = self.rows.get(item.itemData.checked_sub(1)?)?;
                if item.itemID != row.action {
                    return None;
                }
                item.itemWidth = theme.width;
                item.itemHeight = if row.separator {
                    scaled(12, theme.dpi) as u32
                } else {
                    theme.row_height
                };
                Some(1)
            }
            WM_DRAWITEM => {
                let item = &*(lparam as *const DRAWITEMSTRUCT);
                if item.CtlType != ODT_MENU || item.hwndItem.0 != self.handle.0 {
                    return None;
                }
                let row = self.rows.get(item.itemData.checked_sub(1)?)?;
                if item.itemID != row.action {
                    return None;
                }
                theme.draw(row, item);
                Some(1)
            }
            _ => None,
        }
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.handle);
        }
    }
}

#[cfg(test)]
mod tests;

fn high_contrast() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        // If querying accessibility preferences fails, retain the system menu.
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_err()
            || (contrast.dwFlags & HCF_HIGHCONTRASTON).0 != 0
    }
}
fn scaled(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi as i64 + 48) / 96) as i32
}
fn colorref(rgb: u32) -> COLORREF {
    COLORREF(((rgb >> 16) & 0xff) | (rgb & 0xff00) | ((rgb & 0xff) << 16))
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
