pub fn get(name: &str) -> Option<&'static str> {
    match name {
        "selectable_grid" => Some(SELECTABLE_GRID),
        "table" => Some(TABLE),
        "tabs" => Some(TABS),
        "accordion" => Some(ACCORDION),
        "event_timeline" => Some(EVENT_TIMELINE),
        "tree" => Some(TREE),
        "deck" => Some(DECK),
        "nav" => Some(NAV),
        "search" => Some(SEARCH),
        "reload" => Some(RELOAD),
        "source_edit" => Some(SOURCE_EDIT),
        "source_pill" => Some(SOURCE_PILL),
        _ => None,
    }
}

const RELOAD: &str = r#"
(function () {
  if (!/^https?:$/.test(location.protocol)) return;
  var last = null;
  function poll() {
    fetch('/__kazam_version__', { cache: 'no-store' })
      .then(function (r) { return r.ok ? r.text() : null; })
      .then(function (v) {
        if (v === null) return;
        if (last === null) { last = v; return; }
        if (last !== v) location.reload();
      })
      .catch(function () {});
  }
  setInterval(poll, 500);
  poll();
})();
"#;

const NAV: &str = r#"
document.addEventListener('DOMContentLoaded', function () {
  // Active-link highlight (both top nav and sidebar).
  var here = window.location.pathname.replace(/\/$/, '/index.html');
  document.querySelectorAll('.nav-link, .sidebar-link').forEach(function (a) {
    try {
      var raw = a.getAttribute('href');
      if (!raw) return;
      var target = new URL(a.href).pathname.replace(/\/$/, '/index.html');
      if (target === here) {
        a.classList.add('nav-link-active');
        var sub = a.closest('.sidebar-subsection[data-collapsed]');
        if (sub) sub.removeAttribute('data-collapsed');
      }
    } catch (e) {}
  });

  // Sidebar subsection collapse/expand toggle.
  document.querySelectorAll('[data-sidebar-toggle]').forEach(function (label) {
    label.addEventListener('click', function () {
      var sub = label.closest('.sidebar-subsection');
      if (!sub) return;
      if (sub.hasAttribute('data-collapsed')) sub.removeAttribute('data-collapsed');
      else sub.setAttribute('data-collapsed', '');
    });
  });

  // Mobile menu toggle. The button lives inside <nav> and flips `data-open`
  // on that <nav>; CSS does the rest. Escape, outside-click, and link-click
  // all close the panel.
  var toggle = document.querySelector('.nav-menu-toggle');
  if (!toggle) return;
  var nav = toggle.closest('nav');
  if (!nav) return;

  function setOpen(open) {
    if (open) nav.setAttribute('data-open', '');
    else nav.removeAttribute('data-open');
    toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  toggle.addEventListener('click', function (e) {
    e.stopPropagation();
    setOpen(!nav.hasAttribute('data-open'));
  });

  document.addEventListener('click', function (e) {
    if (!nav.hasAttribute('data-open')) return;
    // Closing on any in-panel link click lets navigation feel immediate.
    if (e.target.closest('.site-nav-links a')) {
      setOpen(false);
      return;
    }
    if (!nav.contains(e.target)) setOpen(false);
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape' && nav.hasAttribute('data-open')) {
      setOpen(false);
      toggle.focus();
    }
  });
});
"#;

const SEARCH: &str = r#"
(function () {
  var overlay = document.getElementById('site-search');
  var input = document.getElementById('site-search-input');
  var results = document.getElementById('site-search-results');
  var status = document.getElementById('site-search-status');
  if (!overlay || !input) return;
  var base = input.dataset.base || '';
  var index = null;
  var loadFailed = false;
  var selected = -1;

  function announce(msg) {
    if (status) status.textContent = msg;
  }

  function load() {
    if (index) return Promise.resolve(index);
    results.innerHTML = '<div class="site-search-empty">Loading search index…</div>';
    return fetch(base + 'search.json', { cache: 'default' })
      .then(function (r) {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        return r.json();
      })
      .then(function (data) { index = data.pages || []; loadFailed = false; return index; })
      .catch(function () { loadFailed = true; index = []; return index; });
  }

  function open() {
    overlay.hidden = false;
    input.value = '';
    results.innerHTML = '';
    selected = -1;
    load();
    requestAnimationFrame(function () { input.focus(); });
  }

  function close() {
    overlay.hidden = true;
    input.blur();
  }

  function render(hits) {
    selected = -1;
    if (loadFailed) {
      results.innerHTML = '<div class="site-search-empty">Search is unavailable — the index failed to load. Reload the page to retry.</div>';
      announce('Search unavailable');
      return;
    }
    if (hits.length === 0) {
      var q = input.value.trim();
      results.innerHTML = '<div class="site-search-empty">' +
        (q ? 'No results for “' + esc(q) + '”. Try fewer or different words.' : 'No pages found.') +
        '</div>';
      announce('No results');
      return;
    }
    announce(Math.min(hits.length, 20) + ' result' + (hits.length === 1 ? '' : 's'));
    var html = '';
    for (var i = 0; i < hits.length && i < 20; i++) {
      var h = hits[i];
      var desc = h._matchSnippet || h.description || (h.content_snippets && h.content_snippets[0]) || '';
      if (desc.length > 120) desc = desc.slice(0, 120) + '...';
      html += '<a class="site-search-hit" href="' + base + h.path + '" data-idx="' + i + '">';
      html += '<span class="site-search-hit-title">' + esc(h.title) + '</span>';
      if (desc) html += '<span class="site-search-hit-desc">' + esc(desc) + '</span>';
      html += '</a>';
    }
    results.innerHTML = html;
  }

  function esc(s) {
    var d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  function wordBoundary(hay, term) {
    var i = hay.indexOf(term);
    if (i < 0) return -1;
    if (i === 0 || /\W/.test(hay[i - 1])) return 2;
    return 1;
  }

  function findMatchSnippet(snippets, terms) {
    if (!snippets) return null;
    for (var i = 0; i < snippets.length; i++) {
      var low = snippets[i].toLowerCase();
      var all = true;
      for (var t = 0; t < terms.length; t++) {
        if (low.indexOf(terms[t]) < 0) { all = false; break; }
      }
      if (all) return snippets[i];
    }
    for (var i = 0; i < snippets.length; i++) {
      var low = snippets[i].toLowerCase();
      for (var t = 0; t < terms.length; t++) {
        if (low.indexOf(terms[t]) >= 0) return snippets[i];
      }
    }
    return null;
  }

  var roleFilter = (new URLSearchParams(location.search)).get('role');

  function search(q) {
    if (!index) { render([]); return; }
    var pool = index;
    if (roleFilter) {
      pool = pool.filter(function (p) {
        return !p.personas || p.personas.length === 0 || p.personas.indexOf(roleFilter) >= 0;
      });
    }
    if (!q) { render(pool.slice(0, 10)); return; }
    var terms = q.toLowerCase().split(/\s+/).filter(Boolean);
    var scored = [];
    for (var i = 0; i < pool.length; i++) {
      var p = pool[i];
      var titleL = p.title.toLowerCase();
      var headingsL = (p.headings || []).join(' ').toLowerCase();
      var searchTermsL = (p.search_terms || []).join(' ').toLowerCase();
      var descL = (p.description || '').toLowerCase();
      var snippetsL = (p.content_snippets || []).join(' ').toLowerCase();
      var hay = titleL + ' ' + descL + ' ' + headingsL + ' ' + searchTermsL + ' ' + snippetsL;
      var score = 0;
      var miss = false;
      for (var t = 0; t < terms.length; t++) {
        var term = terms[t];
        if (hay.indexOf(term) < 0) { miss = true; break; }
        var wb = wordBoundary(titleL, term);
        if (wb > 0) { score += wb > 1 ? 10 : 8; }
        else if (wordBoundary(searchTermsL, term) > 0) { score += 8; }
        else if (wordBoundary(headingsL, term) > 0) { score += 5; }
        else if (wordBoundary(descL, term) > 0) { score += 3; }
        else { score += 1; }
      }
      if (!miss) {
        var fs = p.freshness_status;
        if (fs === 'overdue') score -= 3;
        else if (fs === 'expired') score -= 5;
        var hit = Object.assign({}, p);
        var snap = findMatchSnippet(p.content_snippets, terms);
        if (snap) hit._matchSnippet = snap;
        scored.push({ page: hit, score: score });
      }
    }
    scored.sort(function (a, b) { return b.score - a.score; });
    render(scored.map(function (s) { return s.page; }));
  }

  function highlight(n) {
    var hits = results.querySelectorAll('.site-search-hit');
    hits.forEach(function (h, i) { h.classList.toggle('site-search-hit-active', i === n); });
    if (hits[n]) hits[n].scrollIntoView({ block: 'nearest' });
    selected = n;
  }

  document.querySelectorAll('.site-search-btn').forEach(function (btn) {
    btn.addEventListener('click', open);
  });

  overlay.querySelector('.site-search-backdrop').addEventListener('click', close);

  input.addEventListener('input', function () {
    load().then(function () { search(input.value.trim()); });
  });

  overlay.addEventListener('keydown', function (e) {
    var hits = results.querySelectorAll('.site-search-hit');
    if (e.key === 'Escape') { close(); e.preventDefault(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); highlight(Math.min(selected + 1, hits.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); highlight(Math.max(selected - 1, 0)); }
    else if (e.key === 'Enter' && selected >= 0 && hits[selected]) {
      e.preventDefault();
      hits[selected].click();
    }
  });

  document.addEventListener('keydown', function (e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      if (overlay.hidden) open(); else close();
    }
  });
})();
"#;

const SELECTABLE_GRID: &str = r#"
document.querySelectorAll('[data-selectable-grid]').forEach(function (grid) {
  var mode = grid.dataset.interaction || 'single_select';
  var dim = grid.dataset.dimOthers !== 'false';
  var selected = new Set();
  function apply() {
    grid.querySelectorAll('.sel-card, .sel-dot').forEach(function (el) {
      var n = el.dataset.n;
      el.classList.remove('sel-active', 'sel-dimmed');
      if (el.hasAttribute('aria-pressed')) {
        el.setAttribute('aria-pressed', selected.has(n) ? 'true' : 'false');
      }
      if (selected.size === 0) return;
      if (selected.has(n)) el.classList.add('sel-active');
      else if (dim) el.classList.add('sel-dimmed');
    });
  }
  grid.querySelectorAll('.sel-card, .sel-dot').forEach(function (el) {
    el.addEventListener('click', function () {
      var n = el.dataset.n;
      if (mode === 'none') return;
      if (mode === 'single_select') {
        if (selected.has(n)) selected.clear();
        else { selected.clear(); selected.add(n); }
      } else {
        if (selected.has(n)) selected.delete(n); else selected.add(n);
      }
      apply();
    });
  });
});
"#;

const TABLE: &str = r#"
document.querySelectorAll('[data-kazam-table]').forEach(function (table) {
  var tbody = table.tBodies[0];
  var sortState = { col: null, dir: 1 };
  var live = document.createElement('div');
  live.className = 'sr-only';
  live.setAttribute('aria-live', 'polite');
  table.parentElement.appendChild(live);
  function parse(v) {
    var n = parseFloat(v.replace(/[^0-9.\-]/g, ''));
    return isNaN(n) ? v.toLowerCase() : n;
  }
  table.querySelectorAll('th[data-sortable]').forEach(function (th, i) {
    th.tabIndex = 0;
    th.setAttribute('aria-sort', 'none');
    function sort() {
      if (sortState.col === i) sortState.dir = -sortState.dir;
      else { sortState.col = i; sortState.dir = 1; }
      var rows = Array.from(tbody.rows);
      rows.sort(function (a, b) {
        var av = parse(a.cells[i].textContent.trim());
        var bv = parse(b.cells[i].textContent.trim());
        if (av < bv) return -1 * sortState.dir;
        if (av > bv) return 1 * sortState.dir;
        return 0;
      });
      rows.forEach(function (r) { tbody.appendChild(r); });
      table.querySelectorAll('th').forEach(function (h) {
        h.classList.remove('sort-asc', 'sort-desc');
        if (h.hasAttribute('aria-sort')) h.setAttribute('aria-sort', 'none');
      });
      var asc = sortState.dir === 1;
      th.classList.add(asc ? 'sort-asc' : 'sort-desc');
      th.setAttribute('aria-sort', asc ? 'ascending' : 'descending');
      live.textContent = 'Sorted by ' + th.textContent.trim() + ', ' + (asc ? 'ascending' : 'descending');
    }
    th.addEventListener('click', sort);
    th.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); sort(); }
    });
  });
  var filterInput = table.parentElement.querySelector('[data-table-filter]');
  if (filterInput) {
    filterInput.addEventListener('input', function () {
      var q = filterInput.value.toLowerCase();
      var shown = 0;
      Array.from(tbody.rows).forEach(function (r) {
        var hit = r.textContent.toLowerCase().includes(q);
        r.style.display = hit ? '' : 'none';
        if (hit) shown++;
      });
      live.textContent = shown + ' of ' + tbody.rows.length + ' rows shown';
    });
  }
});
"#;

const TABS: &str = r#"
document.querySelectorAll('[data-tabs]').forEach(function (root) {
  var buttons = root.querySelectorAll('.tab-btn');
  var panels = root.querySelectorAll('.tab-panel');
  function show(i, focus) {
    buttons.forEach(function (b, j) {
      b.classList.toggle('tab-btn-active', i === j);
      b.setAttribute('aria-selected', i === j ? 'true' : 'false');
      b.tabIndex = i === j ? 0 : -1;
    });
    panels.forEach(function (p, j) { p.style.display = i === j ? '' : 'none'; });
    if (focus && buttons[i]) buttons[i].focus();
  }
  buttons.forEach(function (b, i) {
    b.addEventListener('click', function () { show(i); });
    b.addEventListener('keydown', function (e) {
      var n = buttons.length;
      if (e.key === 'ArrowRight') { e.preventDefault(); show((i + 1) % n, true); }
      else if (e.key === 'ArrowLeft') { e.preventDefault(); show((i - 1 + n) % n, true); }
      else if (e.key === 'Home') { e.preventDefault(); show(0, true); }
      else if (e.key === 'End') { e.preventDefault(); show(n - 1, true); }
    });
  });
  show(0);
});
"#;

const ACCORDION: &str = r#"
document.querySelectorAll('[data-accordion-item]').forEach(function (item) {
  var btn = item.querySelector('.accordion-head');
  var body = item.querySelector('.accordion-body');
  body.style.display = 'none';
  btn.setAttribute('aria-expanded', 'false');
  btn.addEventListener('click', function () {
    var open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    item.classList.toggle('accordion-open', !open);
    btn.setAttribute('aria-expanded', open ? 'false' : 'true');
  });
});
"#;

const EVENT_TIMELINE: &str = r#"
document.querySelectorAll('[data-event-filter-toggle]').forEach(function (toggle) {
  var timeline = toggle.closest('.c-event-timeline');
  if (!timeline) return;
  toggle.querySelectorAll('button[data-filter]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var val = btn.getAttribute('data-filter');
      timeline.classList.remove('filter-major', 'filter-all');
      timeline.classList.add('filter-' + val);
      timeline.setAttribute('data-filter', val);
      toggle.querySelectorAll('button[data-filter]').forEach(function (b) {
        b.classList.toggle('active', b === btn);
      });
    });
  });
});
document.querySelectorAll('[data-event-tag-filter]').forEach(function (wrap) {
  var timeline = wrap.closest('.c-event-timeline');
  if (!timeline) return;
  var pills = wrap.querySelectorAll('.c-event-tag-pill');
  pills.forEach(function (pill) {
    pill.addEventListener('click', function () {
      pill.classList.toggle('active');
      var active = wrap.querySelectorAll('.c-event-tag-pill.active');
      var tags = [];
      active.forEach(function (a) { tags.push(a.getAttribute('data-tag')); });
      timeline.querySelectorAll('.c-event').forEach(function (ev) {
        var evTags = (ev.getAttribute('data-tags') || '').split(',').filter(Boolean);
        if (tags.length === 0) {
          ev.style.display = '';
        } else {
          var match = tags.some(function (t) { return evTags.indexOf(t) !== -1; });
          ev.style.display = match ? '' : 'none';
        }
      });
    });
  });
});
document.querySelectorAll('[data-event-show-all]').forEach(function (btn) {
  btn.addEventListener('click', function () {
    var timeline = btn.closest('.c-event-timeline');
    if (!timeline) return;
    timeline.querySelectorAll('.c-event-overflow').forEach(function (ev) {
      ev.style.display = '';
      ev.classList.remove('c-event-overflow');
    });
    btn.remove();
  });
});
"#;

const TREE: &str = r#"
document.querySelectorAll('[data-tree-filter-toggle]').forEach(function (toggle) {
  var tree = toggle.closest('.c-tree');
  if (!tree) return;
  var summary = tree.querySelector('[data-tree-summary]');
  var body = tree.querySelector('.c-tree-body');
  toggle.querySelectorAll('button[data-filter]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var val = btn.getAttribute('data-filter');
      toggle.querySelectorAll('button[data-filter]').forEach(function (b) {
        b.classList.toggle('active', b === btn);
      });
      if (val === 'summary') {
        tree.setAttribute('data-view', 'summary');
        if (summary) summary.style.display = '';
        if (body) body.style.display = 'none';
      } else {
        tree.setAttribute('data-view', 'tree');
        if (summary) summary.style.display = 'none';
        if (body) body.style.display = '';
        tree.classList.remove('filter-all', 'filter-incomplete', 'filter-blocked', 'filter-priority');
        tree.classList.add('filter-' + val);
        tree.setAttribute('data-filter', val);
      }
    });
  });
});
document.querySelectorAll('.c-tree').forEach(function (tree) {
  var startCollapsed = tree.classList.contains('c-tree-collapsed');
  var defaultDepth = tree.getAttribute('data-default-depth');
  var maxDepth = defaultDepth ? parseInt(defaultDepth, 10) : null;
  function getDepth(node) {
    var d = 0;
    var el = node.parentElement;
    while (el && !el.classList.contains('c-tree')) {
      if (el.classList.contains('c-tree-node')) d++;
      el = el.parentElement;
    }
    return d;
  }
  tree.querySelectorAll('.c-tree-node').forEach(function (node) {
    var children = node.querySelector(':scope > .c-tree-children');
    if (!children) return;
    if (startCollapsed) {
      node.classList.add('collapsed');
    } else if (maxDepth !== null && getDepth(node) >= maxDepth) {
      node.classList.add('collapsed');
    }
  });
  tree.classList.remove('c-tree-collapsed');
  tree.addEventListener('click', function (e) {
    var chevron = e.target.closest('[data-tree-toggle]');
    if (!chevron) return;
    var node = chevron.closest('.c-tree-node');
    if (node) node.classList.toggle('collapsed');
  });
});
"#;

const DECK: &str = r#"
(function () {
  var track = document.querySelector('.deck-track');
  var slides = document.querySelectorAll('.deck-slide');
  var label = document.getElementById('deck-label');
  var prev = document.getElementById('deck-prev');
  var next = document.getElementById('deck-next');
  var labels = Array.from(slides).map(function (s) { return s.dataset.label; });
  var current = 0;
  var presenting = false;
  var fadeTimer;
  var overlay = document.getElementById('deck-present-overlay');
  var progressBar = document.getElementById('deck-present-progress-bar');
  var counter = document.getElementById('deck-present-counter');
  var presentBtn = document.getElementById('deck-present-btn');
  function fit() {
    slides.forEach(function (slide) {
      var inner = slide.querySelector('.deck-inner');
      if (!inner) return;
      inner.style.transform = '';
      inner.style.transformOrigin = '';
      var availH = slide.clientHeight;
      if (!availH) return;
      var needH = inner.scrollHeight;
      if (!needH) return;
      var k = availH / needH;
      if (k >= 0.99) return;
      k = Math.max(0.4, k);
      inner.style.transformOrigin = 'top center';
      inner.style.transform = 'scale(' + k + ')';
    });
  }
  function updateOverlay() {
    if (!overlay) return;
    if (progressBar) progressBar.style.width = ((current + 1) / slides.length * 100) + '%';
    if (counter) counter.textContent = (current + 1) + ' / ' + slides.length;
  }
  function resetFade() {
    if (!overlay) return;
    overlay.classList.remove('deck-overlay-hidden');
    clearTimeout(fadeTimer);
    fadeTimer = setTimeout(function () { overlay.classList.add('deck-overlay-hidden'); }, 3000);
  }
  function go(n) {
    current = Math.max(0, Math.min(slides.length - 1, n));
    track.style.transform = 'translateX(-' + (current * 100) + '%)';
    label.textContent = labels[current];
    prev.disabled = current === 0;
    if (current > 0) prev.textContent = '← ' + labels[current - 1];
    next.disabled = current === slides.length - 1;
    if (current < slides.length - 1) next.textContent = labels[current + 1] + ' →';
    updateOverlay();
    if (presenting) resetFade();
    requestAnimationFrame(fit);
  }
  function enterPresentation() {
    var root = document.querySelector('.deck-root');
    if (!root) return;
    if (root.requestFullscreen) root.requestFullscreen();
    else if (root.webkitRequestFullscreen) root.webkitRequestFullscreen();
  }
  function exitPresentation() {
    if (document.fullscreenElement) document.exitFullscreen();
    else if (document.webkitFullscreenElement) document.webkitExitFullscreen();
  }
  function onFsChange() {
    var isFs = !!(document.fullscreenElement || document.webkitFullscreenElement);
    presenting = isFs;
    if (isFs) {
      document.body.classList.add('presenting');
      if (overlay) overlay.style.display = '';
      resetFade();
    } else {
      document.body.classList.remove('presenting');
      if (overlay) overlay.style.display = 'none';
      clearTimeout(fadeTimer);
    }
    requestAnimationFrame(fit);
  }
  document.addEventListener('fullscreenchange', onFsChange);
  document.addEventListener('webkitfullscreenchange', onFsChange);
  if (overlay) overlay.style.display = 'none';
  prev.addEventListener('click', function () { go(current - 1); });
  next.addEventListener('click', function () { go(current + 1); });
  if (presentBtn) presentBtn.addEventListener('click', enterPresentation);
  var exitBtn = document.getElementById('deck-present-exit');
  if (exitBtn) exitBtn.addEventListener('click', exitPresentation);
  var overlayPrev = document.getElementById('deck-present-prev');
  var overlayNext = document.getElementById('deck-present-next');
  if (overlayPrev) overlayPrev.addEventListener('click', function () { go(current - 1); });
  if (overlayNext) overlayNext.addEventListener('click', function () { go(current + 1); });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') go(current + 1);
    if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') go(current - 1);
    if ((e.key === 'f' || e.key === 'F') && !presenting && e.target.tagName !== 'INPUT' && e.target.tagName !== 'TEXTAREA') enterPresentation();
  });
  document.addEventListener('mousemove', function () {
    if (presenting) resetFade();
  });
  var fitResizeTimer;
  window.addEventListener('resize', function () {
    clearTimeout(fitResizeTimer);
    fitResizeTimer = setTimeout(fit, 80);
  });
  go(0);
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(fit);
  }
  window.addEventListener('load', fit);
  setTimeout(fit, 100);
})();
"#;

const SOURCE_EDIT: &str = r#"
(function () {
  var marker = document.getElementById('kazam-source-edit');
  if (!marker) return;
  var path = marker.dataset.path;
  var codeBlock = document.querySelector('.c-code');
  if (!codeBlock) return;
  var code = codeBlock.querySelector('code');
  var yaml = code ? code.textContent : '';

  var wrap = document.createElement('div');
  wrap.className = 'source-edit-wrap';

  var textarea = document.createElement('textarea');
  textarea.className = 'source-edit-textarea';
  textarea.value = yaml;
  textarea.spellcheck = false;
  textarea.setAttribute('autocorrect', 'off');
  textarea.setAttribute('autocapitalize', 'off');

  var bar = document.createElement('div');
  bar.className = 'source-edit-bar';

  var saveBtn = document.createElement('button');
  saveBtn.className = 'c-button c-button-primary';
  saveBtn.textContent = 'Save';

  var status = document.createElement('span');
  status.className = 'source-edit-status';
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');

  bar.appendChild(saveBtn);
  bar.appendChild(status);
  wrap.appendChild(bar);
  wrap.appendChild(textarea);
  codeBlock.parentNode.replaceChild(wrap, codeBlock);

  function setStatus(text, kind) {
    status.textContent = text;
    status.className = 'source-edit-status' +
      (kind === 'error' ? ' source-edit-status-error' : kind === 'ok' ? ' source-edit-status-ok' : '');
  }

  function save() {
    setStatus('Saving…');
    saveBtn.disabled = true;
    fetch('/__kazam_write__', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: path, content: textarea.value })
    })
    .then(function (r) {
      if (r.ok) { setStatus('Saved', 'ok'); }
      else {
        r.text().then(function (t) {
          setStatus('Save failed: ' + t + ' — fix the YAML and press Save (or ⌘S) to retry.', 'error');
        });
      }
      saveBtn.disabled = false;
    })
    .catch(function (e) {
      setStatus('Save failed: ' + e.message + ' — is the dev server still running? Retry with Save (or ⌘S).', 'error');
      saveBtn.disabled = false;
    });
  }

  saveBtn.addEventListener('click', save);

  textarea.addEventListener('keydown', function (e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault();
      save();
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      var start = textarea.selectionStart;
      var end = textarea.selectionEnd;
      textarea.value = textarea.value.substring(0, start) + '  ' + textarea.value.substring(end);
      textarea.selectionStart = textarea.selectionEnd = start + 2;
    }
  });

  function resize() {
    textarea.style.height = 'auto';
    textarea.style.height = Math.max(400, textarea.scrollHeight) + 'px';
  }
  textarea.addEventListener('input', resize);
  resize();
})();
"#;

const SOURCE_PILL: &str = r#"
(function () {
  var pill = document.querySelector('.source-pill');
  if (!pill) return;
  var btn = pill.querySelector('.source-pill-btn');

  function items() {
    return Array.from(pill.querySelectorAll('.source-pill-item'));
  }

  function toggle(open) {
    if (open) {
      pill.setAttribute('data-open', '');
      btn.setAttribute('aria-expanded', 'true');
      var first = items()[0];
      if (first) first.focus();
    } else {
      pill.removeAttribute('data-open');
      btn.setAttribute('aria-expanded', 'false');
    }
  }

  btn.addEventListener('click', function (e) {
    e.stopPropagation();
    toggle(!pill.hasAttribute('data-open'));
  });

  document.addEventListener('click', function (e) {
    if (!pill.contains(e.target)) toggle(false);
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape' && pill.hasAttribute('data-open')) {
      toggle(false);
      btn.focus();
    }
  });

  pill.addEventListener('keydown', function (e) {
    if (!pill.hasAttribute('data-open')) return;
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Home' && e.key !== 'End') return;
    var list = items();
    if (list.length === 0) return;
    e.preventDefault();
    var i = list.indexOf(document.activeElement);
    var n;
    if (e.key === 'Home') n = 0;
    else if (e.key === 'End') n = list.length - 1;
    else if (e.key === 'ArrowDown') n = i < 0 ? 0 : (i + 1) % list.length;
    else n = i < 0 ? list.length - 1 : (i - 1 + list.length) % list.length;
    list[n].focus();
  });

  pill.querySelectorAll('[data-copy-prompt]').forEach(function (item) {
    var label = item.lastChild;
    var origText = label.textContent;
    item.addEventListener('click', function () {
      var text = item.getAttribute('data-copy-prompt');
      navigator.clipboard.writeText(text).then(function () {
        item.classList.add('source-pill-copied');
        label.textContent = ' Copied!';
        setTimeout(function () {
          label.textContent = origText;
          item.classList.remove('source-pill-copied');
        }, 1500);
      });
      toggle(false);
    });
  });
})();
"#;
