// Custom Surmount Systems code.
// Released into the public domain under the Unlicense.
// https://unlicense.org

// Parse-time, not deferred: dark first paint before the rest of the document.
document.documentElement.style.colorScheme = "dark";
document.documentElement.style.backgroundColor = "#000000";
document.documentElement.style.color = "#FFFFFF";

(function () {
  var prefetched = Object.create(null);

  function prefetchHref(anchor) {
    if (!anchor || anchor.hasAttribute("download")) {
      return null;
    }

    var raw = anchor.getAttribute("href");
    if (raw == null || raw === "") {
      return null;
    }

    var url;
    try {
      url = new URL(raw, document.baseURI);
    } catch (err) {
      return null;
    }

    var protocol = url.protocol;
    if (
      protocol === "mailto:" ||
      protocol === "tel:" ||
      protocol === "javascript:"
    ) {
      return null;
    }

    if (url.origin !== location.origin) {
      return null;
    }

    // Hash-only: same path and query, so prefetch would not change the document.
    if (url.pathname === location.pathname && url.search === location.search) {
      return null;
    }

    var path = url.pathname;
    var isHtml =
      path.endsWith(".html") ||
      path === "" ||
      path === "/" ||
      path.endsWith("/");
    if (!isHtml) {
      return null;
    }

    url.hash = "";
    return url.href;
  }

  function alreadyHavePrefetchLink(href) {
    var links = document.head.querySelectorAll("link[rel~='prefetch']");
    for (var i = 0; i < links.length; i++) {
      var existing = links[i].getAttribute("href");
      if (existing === href || links[i].href === href) {
        return true;
      }
    }
    return false;
  }

  function prefetch(event) {
    var node = event.target;
    if (!node || !node.closest) {
      return;
    }

    var href = prefetchHref(node.closest("a[href]"));
    if (!href || prefetched[href]) {
      return;
    }

    if (alreadyHavePrefetchLink(href)) {
      prefetched[href] = true;
      return;
    }

    prefetched[href] = true;

    var link = document.createElement("link");
    link.rel = "prefetch";
    link.href = href;
    document.head.appendChild(link);
  }

  // Real navigations only. Do not intercept clicks: MPA view transitions,
  // the support page copy script, and the back button need a full load.
  document.addEventListener("pointerover", prefetch, true);
  document.addEventListener("focusin", prefetch, true);
})();
