use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSString};

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

/// Puts plain text on the general pasteboard.
pub fn copy_text(text: &str) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: the type is an AppKit constant.
    let set =
        pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
    if set {
        Ok(())
    } else {
        Err("cannot copy to the clipboard".into())
    }
}
