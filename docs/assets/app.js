/* ============================================================
 * Vortex 官网 — 交互脚本
 * 无依赖，纯原生。
 * ============================================================ */
(function () {
  'use strict';

  /* ---------- 1. 主题切换 ---------- */
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

  /* ---------- 2. 滚动显现 ---------- */
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

  /* ---------- 3. 代码 Tab 与复制 ---------- */
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

  /* ---------- 4. 界面预览切换 ---------- */
  var SHOT_LABEL = {
    'live-routing': 'vortex — 实时路由',
    'free-tokens':  'vortex — 免费 Token',
    'statistics':   'vortex — 统计',
    'guide':        'vortex — 接入指南',
    'subscriptions': 'vortex — 订阅管理',
    'chat':          'vortex — 对话'
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

  /* ---------- 5. 导航状态与移动端菜单 ---------- */
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

  /* ---------- 6. 下载地址：从最新 Release 解析真实产物 ---------- */
  /* 安装包文件名带版本号（Vortex_0.1.0_x64-setup.exe），写死在 HTML 里必然过期；
     所以静态只给「指向 Releases 页」的兜底链接，运行时再换成真实产物地址。
     API 走 api.github.com，带 CORS 头，file:// 下也能取到。 */
  var DL_REPO = 'ht-shaipe/vortex';
  var DL_RELEASES = 'https://github.com/' + DL_REPO + '/releases/latest';
  var DL_RULES = {
    'windows':   [/[-_]setup\.exe$/i, /\.msi$/i],
    'mac-arm':   [/aarch64\.dmg$/i],
    'mac-intel': [/x64\.dmg$/i],
    'linux':     [/\.AppImage$/i, /\.deb$/i]
  };
  var dlCards = Array.prototype.slice.call(document.querySelectorAll('[data-dl]'));
  var dlNote = document.querySelector('[data-dl-note]');

  function humanSize(bytes) {
    if (!bytes) return '';
    var mb = bytes / 1048576;
    return mb >= 1 ? mb.toFixed(1) + ' MB' : Math.max(1, Math.round(bytes / 1024)) + ' KB';
  }

  if (dlCards.length && window.fetch) {
    fetch('https://api.github.com/repos/' + DL_REPO + '/releases/latest', {
      headers: { Accept: 'application/vnd.github+json' }
    }).then(function (res) {
      if (res.status === 404) return { empty: true };
      if (!res.ok) throw new Error('HTTP ' + res.status);
      return res.json();
    }).then(function (rel) {
      if (rel.empty) {
        // 仓库还没有 Release —— 说清楚，并把出口指向源码构建
        dlCards.forEach(function (c) {
          var f = c.querySelector('[data-dl-file]');
          if (f) f.textContent = '尚未发布预编译版本';
        });
        if (dlNote) dlNote.textContent = '仓库还没有预编译安装包。推一个 v* tag 触发 Release 工作流后，这里的按钮会自动指向真实产物；在那之前可以先从下方源码构建。';
        return;
      }

      var assets = rel.assets || [];
      dlCards.forEach(function (card) {
        var rules = DL_RULES[card.dataset.dl] || [];
        var slots = [card.querySelector('[data-dl-primary]'), card.querySelector('[data-dl-alt]')];
        var matched = [];

        rules.forEach(function (re, i) {
          var hit = null;
          for (var k = 0; k < assets.length; k++) {
            if (re.test(assets[k].name)) { hit = assets[k]; break; }
          }
          var slot = slots[i];
          if (!slot) return;
          if (hit) {
            slot.href = hit.browser_download_url;
            matched.push(hit);
          } else if (i === 0) {
            // 主产物缺失（该平台构建失败/未出包）——不隐藏按钮，改成去 Releases 看
            slot.textContent = '到 Releases 查看';
          } else {
            slot.hidden = true;
          }
        });

        var fileEl = card.querySelector('[data-dl-file]');
        if (fileEl) {
          fileEl.textContent = matched.length
            ? matched[0].name + (matched[0].size ? ' · ' + humanSize(matched[0].size) : '')
            : '本版本未提供该平台产物';
        }
      });

      if (dlNote) {
        var tag = rel.tag_name || rel.name || '';
        var date = rel.published_at ? ' · ' + rel.published_at.slice(0, 10) : '';
        dlNote.textContent = '安装包来自 ' + tag + date + ' 的发布，直接下载即用；全部产物见 Releases 页面。';
      }
    }).catch(function () {
      // 离线或 API 限流：保留指向 Releases 页面的兜底链接，不动文案
    });
  }
})();
