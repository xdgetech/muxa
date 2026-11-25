# 0100: Save, share and drag the Reader's files

Status: accepted (2026-11-25).

## Context

- The host is often a Mac mini. The user reads on another Mac, an iPhone or an iPad, and wants to upload a screenshot or a report from there, for example into a GitHub comment. The host cannot do that upload for them.
- No Apple view had Save or Share. The client already has the bytes: `ReaderContent` carries `text` for a document or a diff and `data` for an image. No server or protocol change is needed.

## Decision

1. **The file's own bytes go out, not the rendered page.** A Markdown or HTML document saves as its `.md` or `.html` source. An image saves as its bytes.
2. **A diff goes out as `<name>.diff`**, with the diff's text. It is not a file in the folder, so the name says what it is. A binary file's diff has no text, so it has no Save or Share.
3. **The Mac has Save… (`⌘S`) and Share in the header of the shown file.** Save opens the system save panel with the file's name. `⌘S` belongs to the button, as Bot Settings' Save does. The two never show together, and the Mac's File menu has no Save item, so nothing else takes the key (ADR 0092 lists the menu keys).
4. **iPhone and iPad have Share.** The share sheet offers Save to Files, and Save Image for an image. iOS shows Save Image only to an app with `NSPhotoLibraryAddUsageDescription`, so the app has it; it adds photos and never reads them.
5. **Drag the file out** on the Mac and the iPad: the title in the header, or the image itself. The drag carries a file, so Finder, Files, Mail and a browser's upload field take it.
6. On the Mac a mouse drag on an image drags the file. It no longer steps to the next image; the arrow keys and buttons still do. A swipe still steps on iOS, where a drag starts with a long press.
7. Share and drag hand out a copy in the app's temporary folder, named as the file. A new version of the file replaces the copy; leaving the file removes it.

## Consequences

- A screenshot an agent made on the host reaches a GitHub comment with one drag on the Mac or the iPad.
- Android has none of this yet. It is a follow-up.
- A whole folder saves and shares as a zip: ADR 0101.

## Alternatives not taken

- **Save the rendered page** (a PDF or the HTML the Reader shows): the user wants the file they would upload, and the source is what a repo or an issue takes.
- **A File › Save menu item for `⌘S`**: a menu item would take the key from Bot Settings' Save button too.
- **Leave diffs out**: a diff is useful to paste into a review, and `.diff` says it is not the file.
