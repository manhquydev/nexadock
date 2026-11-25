(function initTheme() {
  try {
    var saved = localStorage.getItem('nexadock.theme');
    if (!saved) {
      saved = localStorage.getItem('multi-zalo.theme') || localStorage.getItem('color-scheme');
      if (saved === 'light' || saved === 'dark' || saved === 'system') {
        try {
          localStorage.setItem('nexadock.theme', saved);
        } catch (_writeErr) {
          // Ignore storage write errors
        }
      }
    }
    var meta = document.querySelector('meta[name="color-scheme"]');
    if (saved === 'light' || saved === 'dark') {
      if (meta) meta.setAttribute('content', saved);
      document.documentElement.setAttribute('data-theme', saved);
    } else {
      if (meta) meta.setAttribute('content', 'light dark');
      document.documentElement.removeAttribute('data-theme');
    }
    var savedAdv = localStorage.getItem('nexadock.advancedMode') === 'true';
    document.documentElement.setAttribute('data-advanced-mode', savedAdv ? 'true' : 'false');
  } catch (_err) {
    // Ignore storage access errors in restricted contexts
  }
})();
