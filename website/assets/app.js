/* ============================================================
 * Vortex 官网 — 交互脚本
 * 无依赖，纯原生。数据取自 README 的「内置提供商」表。
 * ============================================================ */
(function () {
  'use strict';

  /* ---------- 1. 提供商数据（alias 与免费层标记对齐 README） ---------- */
  var PROVIDERS = [
    { name: 'OpenAI',        alias: 'oa', free: false },
    { name: 'Anthropic',     alias: 'an', free: false },
    { name: 'Google Gemini', alias: 'gm', free: true  },
    { name: 'DeepSeek',      alias: 'ds', free: true  },
    { name: 'Groq',          alias: 'gq', free: true  },
    { name: 'xAI (Grok)',    alias: 'xa', free: false },
    { name: 'Mistral',       alias: 'ml', free: false },
    { name: 'OpenRouter',    alias: 'or', free: true  },
    { name: 'Cohere',        alias: 'ch', free: true  },
    { name: 'Together AI',   alias: 'tg', free: true  },
    { name: 'Fireworks AI',  alias: 'fw', free: false },
    { name: 'Cerebras',      alias: 'cb', free: true  },
    { name: 'NVIDIA NIM',    alias: 'nv', free: true  },
    { name: 'Cloudflare AI', alias: 'cf', free: true  },
    { name: 'Ollama (本地)', alias: 'ol', free: true  },
    { name: 'SiliconFlow',   alias: 'sf', free: true  },
    { name: 'HuggingFace',   alias: 'hf', free: true  },
    { name: 'Pollinations',  alias: 'pl', free: true  },
    { name: 'Perplexity',    alias: 'pp', free: false },
    { name: 'Qwen (通义)',   alias: 'qw', free: true  },
    { name: 'MiniMax',       alias: 'mm', free: false },
    { name: 'Custom 端点',   alias: 'cx', free: false }
  ];

  function chipHTML(p) {
    return '<span class="chip">' +
             '<span class="chip-ico">' + p.alias + '</span>' +
             '<span>' + p.name + '</span>' +
             (p.free ? '<span class="chip-free">免费</span>' : '') +
           '</span>';
  }

  function fillMarquee(el, list) {
    if (!el) return;
    var half = list.map(chipHTML).join('');
    // 轨道内容复制两份，配合 translateX(-50%) 形成无缝循环
    el.innerHTML = half + half;
  }

  var mid = Math.ceil(PROVIDERS.length / 2);
  fillMarquee(document.getElementById('mq1'), PROVIDERS.slice(0, mid));
  fillMarquee(document.getElementById('mq2'), PROVIDERS.slice(mid));

  /* ---------- 2. 主题切换 ---------- */
  var THEME_KEY = 'vortex-site-theme';
  var root = document.documentElement;

  try {
    var saved = localStorage.getItem(THEME_KEY);
    if (saved === 'light' || saved === 'dark') root.setAttribute('data-theme', saved);
  } catch (e) { /* localStorage 不可用时保持默认暗色 */ }

  var themeBtn = document.getElementById('themeBtn');
  if (themeBtn) {
    themeBtn.addEventListener('click', function () {
      var next = root.getAttribute('data-theme') === 'light' ? 'dark' : 'light';
      root.setAttribute('data-theme', next);
      try { localStorage.setItem(THEME_KEY, next); } catch (e) {}
    });
  }

  /* ---------- 3. 滚动显现 ---------- */
  var reveals = Array.prototype.slice.call(document.querySelectorAll('.reveal'));

  if ('IntersectionObserver' in window) {
    // 同一父容器内的元素依次错开入场
    var seen = new WeakMap();
    reveals.forEach(function (el) {
      var p = el.parentElement;
      var n = seen.get(p) || 0;
      seen.set(p, n + 1);
      el.style.transitionDelay = Math.min(n * 55, 330) + 'ms';
    });

    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) {
          en.target.classList.add('in');
          io.unobserve(en.target);
        }
      });
    }, { rootMargin: '0px 0px -8% 0px', threshold: 0.06 });

    reveals.forEach(function (el) { io.observe(el); });
  } else {
    reveals.forEach(function (el) { el.classList.add('in'); });
  }

  /* ---------- 4. 代码 Tab 与复制 ---------- */
  var codeTabs = Array.prototype.slice.call(document.querySelectorAll('.code-tab'));
  var activeLang = 'bash';

  function showCode(lang) {
    activeLang = lang;
    codeTabs.forEach(function (t) { t.classList.toggle('active', t.dataset.code === lang); });
    ['bash', 'python', 'node'].forEach(function (l) {
      var pre = document.getElementById('pre-' + l);
      if (pre) pre.hidden = (l !== lang);
    });
  }

  codeTabs.forEach(function (t) {
    t.addEventListener('click', function () { showCode(t.dataset.code); });
  });

  function fallbackCopy(text) {
    var ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.top = '-1000px';
    document.body.appendChild(ta);
    ta.select();
    var ok = false;
    try { ok = document.execCommand('copy'); } catch (e) { ok = false; }
    document.body.removeChild(ta);
    return ok;
  }

  var copyBtn = document.getElementById('copyBtn');
  if (copyBtn) {
    copyBtn.addEventListener('click', function () {
      var pre = document.getElementById('pre-' + activeLang);
      if (!pre) return;
      var text = pre.textContent;
      var label = copyBtn.querySelector('span');

      function done(ok) {
        label.textContent = ok ? '已复制' : '复制失败';
        copyBtn.classList.toggle('done', ok);
        window.setTimeout(function () {
          label.textContent = '复制';
          copyBtn.classList.remove('done');
        }, 1600);
      }

      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(function () { done(true); },
                                                 function () { done(fallbackCopy(text)); });
      } else {
        done(fallbackCopy(text));
      }
    });
  }

  /* ---------- 5. 界面预览切换 ---------- */
  var SHOT_LABEL = {
    'live-routing': 'vortex — 实时路由',
    'free-tokens':  'vortex — 免费 Token',
    'statistics':   'vortex — 统计',
    'guide':        'vortex — 接入指南'
  };
  var shotTabs = Array.prototype.slice.call(document.querySelectorAll('.shot-tab'));
  var shotImgs = Array.prototype.slice.call(document.querySelectorAll('.shot-img'));
  var shotUrl = document.getElementById('shotUrl');

  shotTabs.forEach(function (tab) {
    tab.addEventListener('click', function () {
      var key = tab.dataset.shot;
      shotTabs.forEach(function (t) { t.classList.toggle('active', t === tab); });
      shotImgs.forEach(function (img) { img.hidden = img.dataset.shot !== key; });
      if (shotUrl && SHOT_LABEL[key]) shotUrl.textContent = SHOT_LABEL[key];
    });
  });

  /* ---------- 6. 导航状态与移动端菜单 ---------- */
  var nav = document.getElementById('nav');
  var burger = document.getElementById('burger');
  var navLinks = document.getElementById('navLinks');

  function onScroll() {
    if (nav) nav.classList.toggle('scrolled', window.scrollY > 8);
  }
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  if (burger && navLinks) {
    burger.addEventListener('click', function () {
      var open = navLinks.classList.toggle('open');
      burger.setAttribute('aria-expanded', open ? 'true' : 'false');
    });
    navLinks.addEventListener('click', function (e) {
      if (e.target.tagName === 'A') {
        navLinks.classList.remove('open');
        burger.setAttribute('aria-expanded', 'false');
      }
    });
  }
})();
