use super::*;
use windows::Win32::UI::Controls::{ODA_DRAWENTIRE, ODS_FLAGS};

fn items(hotkey: &str) -> Vec<Item> {
    vec![
        Item {
            action: 1,
            label: format!("开始录音\t{hotkey}"),
            enabled: true,
            separator: false,
        },
        Item {
            action: 2,
            label: "取消本次录音".into(),
            enabled: false,
            separator: false,
        },
        Item {
            action: 0,
            label: String::new(),
            enabled: false,
            separator: true,
        },
    ]
}

struct Surface {
    dc: HDC,
    bitmap: GdiObject,
    original: HGDIOBJ,
}
impl Surface {
    unsafe fn new(width: i32, height: i32) -> Self {
        let screen = WindowDc(HWND::default(), GetDC(None));
        let dc = CreateCompatibleDC(Some(screen.1));
        assert!(!dc.is_invalid());
        let bitmap =
            GdiObject::new(HGDIOBJ(CreateCompatibleBitmap(screen.1, width, height).0)).unwrap();
        let original = SelectObject(dc, bitmap.0);
        Self {
            dc,
            bitmap,
            original,
        }
    }
    unsafe fn rgb_count(&self, width: i32, height: i32, color: u32) -> usize {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|(x, y)| GetPixel(self.dc, *x, *y) == colorref(color))
            .count()
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.original);
            let _ = DeleteDC(self.dc);
        }
        // The bitmap is released only after it is deselected.
        let _ = &self.bitmap;
    }
}

#[test]
fn native_gdi_draws_the_shared_palette_at_three_dpis() {
    unsafe {
        for dpi in [96, 144, 192] {
            let menu = Popup::create(0, dpi, items("Ctrl+Alt+Shift+Super+F24"), false).unwrap();
            let theme = menu.theme.as_ref().unwrap();
            let (width, height) = (theme.width as i32, theme.row_height as i32);
            let surface = Surface::new(width, height);
            let mut draw = DRAWITEMSTRUCT {
                CtlType: ODT_MENU,
                itemID: 1,
                itemData: 1,
                hwndItem: HWND(menu.handle.0),
                hDC: surface.dc,
                itemAction: ODA_DRAWENTIRE,
                rcItem: RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                ..Default::default()
            };
            assert_eq!(
                menu.message(WM_DRAWITEM, 0, (&draw as *const DRAWITEMSTRUCT) as isize),
                Some(1)
            );
            assert_eq!(GetPixel(surface.dc, 0, 0), colorref(palette::PANEL));
            assert!(surface.rgb_count(width, height, palette::TEXT) > 10);
            assert!(
                surface.rgb_count(width, height, palette::MUTED) > 10,
                "shortcut hint missing"
            );
            draw.itemState = ODS_SELECTED;
            menu.message(WM_DRAWITEM, 0, (&draw as *const DRAWITEMSTRUCT) as isize);
            assert_eq!(
                GetPixel(surface.dc, scaled(8, dpi), height / 2),
                colorref(palette::TINT)
            );
            assert!(surface.rgb_count(width, height, palette::MINT) > 10);
            draw.itemID = 2;
            draw.itemData = 2;
            draw.itemState = ODS_FLAGS(ODS_SELECTED.0 | ODS_DISABLED.0);
            menu.message(WM_DRAWITEM, 0, (&draw as *const DRAWITEMSTRUCT) as isize);
            assert_eq!(surface.rgb_count(width, height, palette::TINT), 0);
            assert!(surface.rgb_count(width, height, palette::MUTED) > 10);
            draw.itemID = 0;
            draw.itemData = 3;
            menu.message(WM_DRAWITEM, 0, (&draw as *const DRAWITEMSTRUCT) as isize);
            assert_eq!(
                GetPixel(surface.dc, width / 2, height / 2),
                colorref(palette::LINE)
            );
            assert_eq!(GetPixel(surface.dc, width / 2, 0), colorref(palette::PANEL));
            let short = Popup::create(0, dpi, items("F8"), false).unwrap();
            assert!(theme.width > short.theme.as_ref().unwrap().width);
            let mut measure = MEASUREITEMSTRUCT {
                CtlType: ODT_MENU,
                itemID: 1,
                itemData: 1,
                ..Default::default()
            };
            assert_eq!(
                menu.message(
                    WM_MEASUREITEM,
                    0,
                    (&mut measure as *mut MEASUREITEMSTRUCT) as isize
                ),
                Some(1)
            );
            assert!(measure.itemHeight >= scaled(40, dpi) as u32);
            assert_eq!(measure.itemWidth, theme.width);
            // Reject stale/foreign item data instead of dereferencing it.
            measure.itemData = usize::MAX;
            assert_eq!(
                menu.message(
                    WM_MEASUREITEM,
                    0,
                    (&mut measure as *mut MEASUREITEMSTRUCT) as isize
                ),
                None
            );
        }
    }
}

#[test]
fn menu_keeps_accessible_labels_and_system_fallback_without_ownerdraw() {
    unsafe {
        for system_colors in [false, true] {
            let menu = Popup::create(0, 144, items("Ctrl+Alt+F24"), system_colors).unwrap();
            let mut text = [0u16; 128];
            let mut info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_STRING | MIIM_STATE | MIIM_FTYPE,
                dwTypeData: PWSTR(text.as_mut_ptr()),
                cch: text.len() as u32,
                ..Default::default()
            };
            GetMenuItemInfoW(menu.handle, 0, true, &mut info).unwrap();
            assert_eq!(
                String::from_utf16_lossy(&text[..info.cch as usize]),
                "开始录音\tCtrl+Alt+F24"
            );
            assert_eq!((info.fType & MFT_OWNERDRAW).0 != 0, !system_colors);
            GetMenuItemInfoW(menu.handle, 1, true, &mut info).unwrap();
            assert_ne!((info.fState & MFS_DISABLED).0, 0);
            GetMenuItemInfoW(menu.handle, 2, true, &mut info).unwrap();
            assert_ne!((info.fType & MFT_SEPARATOR).0, 0);
        }
    }
}
