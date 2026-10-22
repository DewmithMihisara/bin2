/* bin2 viewer. Inlined into every .bin2 file. No network, no dependencies. */
(function () {
  "use strict";

  var $ = function (id) { return document.getElementById(id); };
  var manifest = JSON.parse($("bin2-manifest").textContent);
  var stage = $("bin2-stage");
  var sub = $("bin2-sub");

  function el(tag, attrs, text) {
    var node = document.createElement(tag);
    if (attrs) for (var k in attrs) node.setAttribute(k, attrs[k]);
    if (text != null) node.textContent = text;
    return node;
  }

  function formatSize(n) {
    var units = ["bytes", "KB", "MB", "GB"];
    var i = 0;
    while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
    return (i === 0 ? n : n.toFixed(1)) + " " + units[i];
  }

  function base64ToBytes(s) {
    s = s.replace(/\s+/g, "");
    if (typeof Uint8Array.fromBase64 === "function") return Uint8Array.fromBase64(s);
    var bin = atob(s);
    var out = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  function readPayloadText() {
    var node = $("bin2-payload");
    if (node.tagName === "IMG") {
      var src = node.getAttribute("src");
      return src.slice(src.indexOf(",") + 1);
    }
    return node.textContent;
  }

  async function decodeOriginal() {
    var raw = base64ToBytes(readPayloadText());
    if (manifest.encoding === "gzip+base64") {
      var stream = new Blob([raw]).stream().pipeThrough(new DecompressionStream("gzip"));
      return new Uint8Array(await new Response(stream).arrayBuffer());
    }
    return raw;
  }

  async function sha256Hex(bytes) {
    if (!(window.crypto && crypto.subtle)) return null;
    var digest = await crypto.subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest), function (b) {
      return b.toString(16).padStart(2, "0");
    }).join("");
  }

  /* ---------- renderers ---------- */

  function renderText(bytes) {
    var wrap = el("div", { class: "doc" });
    wrap.appendChild(el("pre", { class: "text" }, new TextDecoder().decode(bytes)));
    return wrap;
  }

  function renderMarkdown(bytes) {
    var wrap = el("div", { class: "doc" });
    wrap.innerHTML = markdownToHtml(new TextDecoder().decode(bytes));
    return wrap;
  }

  function renderHtml(bytes) {
    /* Sandboxed with no permissions: the page shows, but its scripts never run. */
    var frame = el("iframe", { class: "frame", sandbox: "" });
    frame.srcdoc = new TextDecoder().decode(bytes);
    return frame;
  }

  function renderHex(bytes) {
    var wrap = el("div", { class: "doc" });
    var msg = el("div", { class: "msg" });
    msg.appendChild(el("h2", null, "No viewer for this type yet"));
    msg.appendChild(el("p", null,
      "The original file is safe inside this bin2. Use “Download original” to get it back. " +
      "Below are its first bytes."));
    wrap.appendChild(msg);
    var lines = [];
    var limit = Math.min(bytes.length, 1024);
    for (var off = 0; off < limit; off += 16) {
      var hex = "", ascii = "";
      for (var j = 0; j < 16; j++) {
        if (off + j < limit) {
          var b = bytes[off + j];
          hex += b.toString(16).padStart(2, "0") + " ";
          ascii += b >= 32 && b < 127 ? String.fromCharCode(b) : ".";
        } else {
          hex += "   ";
        }
      }
      lines.push(off.toString(16).padStart(8, "0") + "  " + hex + " " + ascii);
    }
    if (bytes.length > limit) lines.push("… " + formatSize(bytes.length - limit) + " more");
    wrap.appendChild(el("pre", { class: "hex" }, lines.join("\n")));
    return wrap;
  }

  function render(bytes, url) {
    switch (manifest.kind) {
      case "image":
        /* Images are already shown by the static <img>. Nothing to add. */
        return null;
      case "pdf":
        return el("iframe", { class: "frame", src: url, title: manifest.name });
      case "video":
        return el("video", { class: "media", src: url, controls: "" });
      case "audio":
        return el("audio", { class: "media", src: url, controls: "" });
      case "markdown":
        return renderMarkdown(bytes);
      case "html":
        return renderHtml(bytes);
      case "text":
        return renderText(bytes);
      default:
        return renderHex(bytes);
    }
  }

  /* ---------- minimal, safe Markdown ---------- */

  function escapeHtml(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  function inline(s) {
    s = escapeHtml(s);
    s = s.replace(/`([^`]+)`/g, "<code>$1</code>");
    s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
    s = s.replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
    s = s.replace(/\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)/g, function (m, label, href) {
      if (!/^(https?:|mailto:|#)/i.test(href)) return label;
      return '<a href="' + href + '" target="_blank" rel="noopener noreferrer">' + label + "</a>";
    });
    return s;
  }

  function markdownToHtml(src) {
    var lines = src.replace(/\r\n?/g, "\n").split("\n");
    var out = [], para = [], list = null, i = 0;

    function flushPara() {
      if (para.length) { out.push("<p>" + inline(para.join(" ")) + "</p>"); para = []; }
    }
    function flushList() {
      if (list) { out.push("</" + list + ">"); list = null; }
    }

    while (i < lines.length) {
      var line = lines[i];
      var m;
      if (/^```/.test(line)) {
        flushPara(); flushList();
        var code = [];
        i++;
        while (i < lines.length && !/^```/.test(lines[i])) { code.push(lines[i]); i++; }
        out.push("<pre><code>" + escapeHtml(code.join("\n")) + "</code></pre>");
        i++;
        continue;
      }
      if ((m = /^(#{1,6})\s+(.*)$/.exec(line))) {
        flushPara(); flushList();
        out.push("<h" + m[1].length + ">" + inline(m[2]) + "</h" + m[1].length + ">");
      } else if ((m = /^\s*[-*+]\s+(.*)$/.exec(line)) || (m = /^\s*\d+[.)]\s+(.*)$/.exec(line))) {
        flushPara();
        var type = /^\s*\d/.test(line) ? "ol" : "ul";
        if (list !== type) { flushList(); out.push("<" + type + ">"); list = type; }
        out.push("<li>" + inline(m[1]) + "</li>");
      } else if ((m = /^>\s?(.*)$/.exec(line))) {
        flushPara(); flushList();
        out.push("<blockquote>" + inline(m[1]) + "</blockquote>");
      } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
        flushPara(); flushList();
        out.push("<hr>");
      } else if (/^\s*$/.test(line)) {
        flushPara(); flushList();
      } else {
        flushList();
        para.push(line.trim());
      }
      i++;
    }
    flushPara(); flushList();
    return out.join("\n");
  }

  /* ---------- info panel ---------- */

  function buildInfo(verified) {
    var panel = $("bin2-info");
    var rows = [
      ["Name", manifest.name],
      ["Type", manifest.mime],
      ["Size", formatSize(manifest.size)],
      ["Sealed", manifest.sealed_at],
      ["SHA-256", manifest.sha256],
      ["Integrity", verified === true ? "Verified" : verified === false ? "MISMATCH: file may be damaged" : "Not checked"],
      ["Encoding", manifest.encoding],
      ["Made with", manifest.tool]
    ];
    var table = el("table");
    rows.forEach(function (r) {
      var tr = el("tr");
      tr.appendChild(el("td", null, r[0]));
      tr.appendChild(el("td", null, String(r[1])));
      table.appendChild(tr);
    });
    panel.replaceChildren(table);
    $("bin2-info-btn").addEventListener("click", function () { panel.hidden = !panel.hidden; });
  }

  /* ---------- start ---------- */

  async function main() {
    var bytes;
    try {
      bytes = await decodeOriginal();
    } catch (err) {
      stage.replaceChildren(el("div", { class: "center msg" },
        "This bin2 file could not be opened: " + err.message));
      return;
    }

    var blob = new Blob([bytes], { type: manifest.mime });
    var url = URL.createObjectURL(blob);

    var download = $("bin2-download");
    download.href = url;
    download.download = manifest.name;
    $("bin2-actions").hidden = false;

    var view = render(bytes, url);
    if (view) stage.replaceChildren(view);

    var verified = null;
    try {
      var hex = await sha256Hex(bytes);
      if (hex) verified = hex === manifest.sha256;
    } catch (e) { /* crypto unavailable in this context */ }

    sub.replaceChildren(document.createTextNode(formatSize(manifest.size) + " · " + manifest.mime + " · "));
    if (verified === true) sub.appendChild(el("span", { class: "ok" }, "✓ verified"));
    else if (verified === false) sub.appendChild(el("span", null, "⚠ damaged"));
    else sub.appendChild(el("span", null, "sealed " + manifest.sealed_at.slice(0, 10)));

    buildInfo(verified);
  }

  main();
})();
