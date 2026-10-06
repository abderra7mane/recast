/** Whether keys pressed in `target` type text rather than run shortcuts. */
export function isTyping(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement)
    return true;
  return (
    target instanceof HTMLInputElement &&
    !["checkbox", "radio", "range", "color", "button"].includes(target.type)
  );
}
