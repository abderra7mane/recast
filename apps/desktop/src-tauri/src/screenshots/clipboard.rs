use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypePNG, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData};

/// Puts a PNG image on the general pasteboard.
pub fn copy_png(png: &[u8]) -> Result<(), String> {
    let item = NSPasteboardItem::new();
    // SAFETY: the type is an AppKit constant.
    let set = item.setData_forType(&NSData::with_bytes(png), unsafe { NSPasteboardTypePNG });
    if !set {
        return Err("cannot prepare the image for the clipboard".into());
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let writer: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
        ProtocolObject::from_retained(item);
    if pasteboard.writeObjects(&NSArray::from_retained_slice(&[writer])) {
        Ok(())
    } else {
        Err("cannot copy the image to the clipboard".into())
    }
}
