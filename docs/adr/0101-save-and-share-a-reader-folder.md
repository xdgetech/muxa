# 0101: Save and share a Reader folder

Status: accepted (2026-11-25).

## Context

- ADR 0100 saves, shares and drags the one Reader file on screen. The client already has its bytes.
- The user also wants a whole folder, for example a spec-qa run `output/qa/<ticket>/<run>/` with `report.md` and its screenshots. The host is often a Mac mini; the upload to GitHub happens from another Mac, an iPhone or an iPad.
- The client has not opened those files, so the daemon must send them.

## Decision

1. **One zip in one reply.** `reader-folder` `{"terminal","folder"}` returns `{"zip"}`, a zip in base64.
   - A browser upload field, a GitHub comment, Mail and the iOS share sheet take one file. Loose files do not drag into a browser.
   - The zip has one top folder named after the folder, so it unpacks to `<run>/report.md`, `<run>/screens/1.png`.
   - Entries are stored, not compressed. Images are compressed already, and documents are small.
2. **Only what the Reader lists.** The zip holds the files that the Docs list (with HTML) and the Images list hold under `folder/`, subfolders included. Empty `folder` means every listed file.
   - The daemon never opens `folder` as a path. It only compares it with listed paths, so `..`, `/x` or `out` for `output/` match nothing.
   - Every ADR 0099 rule still holds: no dot names, no dependency folders, no secret-like names, no file types other than documents and images.
   - Each file is read with the Reader's own checks: the real file stays inside the repo, at most 2 MiB for a document.
   - A diff is not a file of the folder, so the Changes list has no folder action.
3. **Limits refuse the whole folder; nothing is cut.** At most 11 MB of files (the size of the largest image, so the base64 fits one 16 MiB frame) and 1000 files. Past a limit, or with no listed files, the reply is `bad-request` with a message the apps show as is: "output/qa has 23 MB of files. Save Folder sends at most 11 MB. Save a folder inside it."
4. **The apps (iPhone, iPad, Mac).**
   - A folder row's menu has Save Folder… (the Mac) and Share Folder. On the Mac and the iPad the row drags out as the zip.
   - A file row's menu offers the same for each folder above the file. A folder with one file has no row of its own, and a run folder often has one report.
   - The zip is named after the folder and is fetched only when the user saves, shares or drags it.
5. **Android: a follow-up**, with ADR 0100's single-file save. The Kotlin client has the request already.

## Consequences

- A spec-qa run reaches a GitHub comment as one zip with one drag on the Mac.
- A folder over 11 MB needs another way, such as a folder inside it or `scp`. Sending it in parts would need a chunked reply; it can come when 11 MB proves too small.
- `muxa-cli v2 reader-folder TERMINAL FOLDER OUT.zip` writes the same zip.

## Alternatives not taken

- **The files one by one:** the client would have to build a folder or a zip itself, and a browser takes neither a folder drop nor many files from one drag everywhere.
- **Cut the zip at the limit:** a report without some of its screenshots looks complete and is not.
- **Compression:** a codec dependency for little gain on images.
