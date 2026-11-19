// Shows one HTML document in the sandboxed frame. The app calls show() again
// with the whole text when the file changes; the document then starts over.
function show(text) {
  document.getElementById("doc").srcdoc = text;
}
