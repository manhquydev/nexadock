(function initTheme() {
  try {
    var saved = localStorage.getItem('color-scheme');
    var meta = document.querySelector('meta[name="color-scheme"]');
    if (saved === 'light' || saved === 'dark') {
      if (meta) meta.setAttribute('content', saved);
      document.documentElement.setAttribute('data-theme', saved);
    } else {
      if (meta) meta.setAttribute('content', 'light dark');
      document.documentElement.setAttribute('data-theme', 'system');
    }
  } catch (_err) {
    // Ignore storage access errors in restricted contexts
  }
})();
