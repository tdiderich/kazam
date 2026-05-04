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
        "reload" => Some(RELOAD),
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
      if (target === here) a.classList.add('nav-link-active');
    } catch (e) {}
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

const SELECTABLE_GRID: &str = r#"
document.querySelectorAll('[data-selectable-grid]').forEach(function (grid) {
  var mode = grid.dataset.interaction || 'single_select';
  var dim = grid.dataset.dimOthers !== 'false';
  var selected = new Set();
  function apply() {
    grid.querySelectorAll('.sel-card, .sel-dot').forEach(function (el) {
      var n = el.dataset.n;
      el.classList.remove('sel-active', 'sel-dimmed');
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
  function parse(v) {
    var n = parseFloat(v.replace(/[^0-9.\-]/g, ''));
    return isNaN(n) ? v.toLowerCase() : n;
  }
  table.querySelectorAll('th[data-sortable]').forEach(function (th, i) {
    th.addEventListener('click', function () {
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
      });
      th.classList.add(sortState.dir === 1 ? 'sort-asc' : 'sort-desc');
    });
  });
  var filterInput = table.parentElement.querySelector('[data-table-filter]');
  if (filterInput) {
    filterInput.addEventListener('input', function () {
      var q = filterInput.value.toLowerCase();
      Array.from(tbody.rows).forEach(function (r) {
        r.style.display = r.textContent.toLowerCase().includes(q) ? '' : 'none';
      });
    });
  }
});
"#;

const TABS: &str = r#"
document.querySelectorAll('[data-tabs]').forEach(function (root) {
  var buttons = root.querySelectorAll('.tab-btn');
  var panels = root.querySelectorAll('.tab-panel');
  function show(i) {
    buttons.forEach(function (b, j) { b.classList.toggle('tab-btn-active', i === j); });
    panels.forEach(function (p, j) { p.style.display = i === j ? '' : 'none'; });
  }
  buttons.forEach(function (b, i) { b.addEventListener('click', function () { show(i); }); });
  show(0);
});
"#;

const ACCORDION: &str = r#"
document.querySelectorAll('[data-accordion-item]').forEach(function (item) {
  var btn = item.querySelector('.accordion-head');
  var body = item.querySelector('.accordion-body');
  body.style.display = 'none';
  btn.addEventListener('click', function () {
    var open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    item.classList.toggle('accordion-open', !open);
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
"#;

const TREE: &str = r#"
document.querySelectorAll('[data-tree-filter-toggle]').forEach(function (toggle) {
  var tree = toggle.closest('.c-tree');
  if (!tree) return;
  toggle.querySelectorAll('button[data-filter]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var val = btn.getAttribute('data-filter');
      tree.classList.remove('filter-all', 'filter-incomplete', 'filter-blocked', 'filter-priority');
      tree.classList.add('filter-' + val);
      tree.setAttribute('data-filter', val);
      toggle.querySelectorAll('button[data-filter]').forEach(function (b) {
        b.classList.toggle('active', b === btn);
      });
    });
  });
});
document.querySelectorAll('.c-tree').forEach(function (tree) {
  var startCollapsed = tree.classList.contains('c-tree-collapsed');
  tree.querySelectorAll('.c-tree-node').forEach(function (node) {
    var children = node.querySelector(':scope > .c-tree-children');
    if (!children) return;
    if (startCollapsed) node.classList.add('collapsed');
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
  function fit() {
    slides.forEach(function (slide) {
      var inner = slide.querySelector('.deck-inner');
      if (!inner) return;
      // Reset any previous transform so we measure natural content.
      inner.style.transform = '';
      inner.style.transformOrigin = '';
      var availH = slide.clientHeight;
      if (!availH) return;
      var needH = inner.scrollHeight;
      if (!needH) return;
      var k = availH / needH;
      if (k >= 0.99) return; // already fits, leave at natural size
      k = Math.max(0.4, k);
      inner.style.transformOrigin = 'top center';
      inner.style.transform = 'scale(' + k + ')';
    });
  }
  function go(n) {
    current = Math.max(0, Math.min(slides.length - 1, n));
    track.style.transform = 'translateX(-' + (current * 100) + '%)';
    label.textContent = labels[current];
    if (current === 0) { prev.style.visibility = 'hidden'; }
    else { prev.style.visibility = 'visible'; prev.textContent = '← ' + labels[current - 1]; }
    if (current === slides.length - 1) { next.style.visibility = 'hidden'; }
    else { next.style.visibility = 'visible'; next.textContent = labels[current + 1] + ' →'; }
    // Re-fit in case the just-revealed slide measured 0 while hidden.
    requestAnimationFrame(fit);
  }
  prev.addEventListener('click', function () { go(current - 1); });
  next.addEventListener('click', function () { go(current + 1); });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') go(current + 1);
    if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') go(current - 1);
  });
  var fitTimer;
  window.addEventListener('resize', function () {
    clearTimeout(fitTimer);
    fitTimer = setTimeout(fit, 80);
  });
  go(0);
  // Wait for fonts/images to settle before measuring.
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(fit);
  }
  window.addEventListener('load', fit);
  setTimeout(fit, 100);
})();
"#;
