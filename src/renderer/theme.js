'use strict';

// Loaded in <head> so a stored theme is in place before first paint (no light-to-dark flip on launch).
try {
  const saved = localStorage.getItem('geo-theme');
  if (saved === 'light' || saved === 'dark') document.documentElement.setAttribute('data-theme', saved);
} catch { /* storage unavailable: follow the system theme */ }
