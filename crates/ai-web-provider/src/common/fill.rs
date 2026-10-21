//! contenteditable / textarea 多策略文本填入（平台无关）。
//!
//! 目标站点输入框多为 tiptap/ProseMirror 或 Slate 编辑器，obscura 的
//! execCommand 是空桩，直接改 DOM 不会同步编辑器内部状态（发送按钮保持
//! disabled）。按以下策略依次尝试，返回 `ok-*`（均通过 DOM 事后校验）
//! 或失败原因：
//! 0. Slate 编辑器（data-slate-editor）：首选 fiber BFS 找 Slate editor
//!    对象直接 insertText 写模型；退化手段为伪造 getTargetRanges 的
//!    beforeinput。渲染走 React setState 是异步的，无法同步校验，返回
//!    `slate-pending` 由调用方 settle 后校验
//! 1. React fiber 找 ProseMirror view.dispatch（直接操作内部状态）
//! 2. React fiber 找 tiptap editor.commands.insertContent
//! 3. beforeinput 事件（ProseMirror 原生处理，不修改 DOM 让它自己插入）
//! 4. clipboard paste 事件（ProseMirror 原生处理）
//! 5. pmViewDesc 原始路径找 ProseMirror view
//! 6. execCommand / React textarea（native setter + InputEvent + fiber onChange）
//! 7. DOM 手动插入（兜底，状态可能不同步导致发送按钮 disabled）

/// 多策略填入 JS（IIFE，接受 text 参数；由 build_fill_js 拼接实参）
const FILL_JS_BODY: &str = r#"((text) => {
  const el = document.querySelector('.ProseMirror[contenteditable="true"]')
    || document.querySelector('[contenteditable="true"]')
    || document.querySelector('textarea');
  if (!el) return 'no-input';
  el.focus();

  const verify = () => (el.textContent || el.value || '').trim().includes(text);

  // 深度查找 Slate editor 对象（BFS 同 findView）：
  // Slate editor 是 children 为数组的对象且 apply/insertText/isInline 均为
  // 函数——该组合不会误中 ProseMirror view（无 children 数组）、React fiber
  // （child/sibling 为单数）或 Slate 文档节点（无 apply/insertText）
  const findSlate = (roots) => {
    const seen = new Set();
    const queue = [...roots];
    while (queue.length) {
      const node = queue.shift();
      if (!node || seen.has(node) || seen.size > 2000) continue;
      seen.add(node);
      for (const k of Object.keys(node)) {
        try {
          const v = node[k];
          if (v && typeof v === 'object') {
            if (Array.isArray(v.children) && typeof v.apply === 'function'
                && typeof v.insertText === 'function' && typeof v.isInline === 'function') {
              return v;
            }
            queue.push(v);
          }
        } catch(e) {}
      }
      for (const k of Object.getOwnPropertyNames(node)) {
        if (k.startsWith('__reactFiber') || k.startsWith('__reactInternalInstance')) {
          try {
            const fiber = node[k];
            if (fiber) {
              queue.push(fiber);
              if (fiber.memoizedProps) queue.push(fiber.memoizedProps);
              if (fiber.memoizedState) queue.push(fiber.memoizedState);
            }
          } catch(e) {}
        }
      }
    }
    return null;
  };

  // 策略0: Slate 编辑器（data-slate-editor，如千问）。
  // DOM 手动插入无法同步 Slate 状态（发送按钮保持 disabled）；Slate 的
  // 渲染走 React setState（异步），fill 阶段无法同步 verify，故两种手段
  // 成功执行后统一返回 slate-pending，由调用方 settle 后另行校验。
  try {
    const slateEl = el.closest && el.closest('[data-slate-editor="true"]');
    if (slateEl) {
      let handled = false;
      // 手段1（首选）: fiber BFS 找 Slate editor 对象，直接 insertText 写
      // 内部模型并触发 onChange——绕过 DOM 事件管线，不依赖站点对合成
      // beforeinput 的处理（与 ProseMirror fiber 策略同款思路）
      try {
        const editor = findSlate([el, el.parentElement, document.body]);
        if (editor) {
          editor.insertText(text);
          handled = true;
          if (verify()) return 'ok-slate-fiber';
        }
      } catch(e) {}
      // 手段2（退化）: 合成 beforeinput。Slate React 依据
      // getTargetRanges() 定位插入点，合成事件默认返回空数组会被直接
      // 忽略——伪造 StaticRange（DOM Range 兼容其字段），同时设置真实
      // DOM 选区以覆盖只读 live selection 的版本
      if (!handled) {
        const r = document.createRange();
        const ps = slateEl.querySelectorAll('p[data-slate-node="element"]');
        const last = ps.length ? ps[ps.length - 1] : null;
        if (last && last.firstChild) {
          const t = last.firstChild;
          r.setStart(t, (t.textContent || '').length);
          r.setEnd(t, (t.textContent || '').length);
        } else {
          r.selectNodeContents(slateEl);
          r.collapse(false);
        }
        try {
          const sel = window.getSelection();
          sel.removeAllRanges();
          sel.addRange(r);
        } catch(e) {}
        const evt = new InputEvent('beforeinput', {
          inputType: 'insertText', data: text,
          bubbles: true, cancelable: true
        });
        evt.getTargetRanges = () => [r];
        slateEl.dispatchEvent(evt);
        if (verify()) return 'ok-slate-sync';
      }
      // 不能继续走后续策略：可能与 Slate 的异步渲染叠加造成双重插入
      return 'slate-pending';
    }
  } catch(e) {}

  // 深度查找 ProseMirror view / tiptap editor：
  // 遍历元素及其祖先和 document.body 的所有属性（含 React fiber）
  const findView = (roots) => {
    const seen = new Set();
    const queue = [...roots];
    while (queue.length) {
      const node = queue.shift();
      if (!node || seen.has(node) || seen.size > 2000) continue;
      seen.add(node);
      for (const k of Object.keys(node)) {
        try {
          const v = node[k];
          if (v && typeof v === 'object') {
            if (v.view && v.state && typeof v.dispatch === 'function') return v.view;
            queue.push(v);
          }
        } catch(e) {}
      }
      for (const k of Object.getOwnPropertyNames(node)) {
        if (k.startsWith('__reactFiber') || k.startsWith('__reactInternalInstance')) {
          try {
            const fiber = node[k];
            if (fiber) {
              queue.push(fiber);
              if (fiber.memoizedProps) queue.push(fiber.memoizedProps);
              if (fiber.memoizedState) queue.push(fiber.memoizedState);
            }
          } catch(e) {}
        }
      }
    }
    return null;
  };

  const findEditor = (roots) => {
    const seen = new Set();
    const queue = [...roots];
    while (queue.length) {
      const node = queue.shift();
      if (!node || seen.has(node) || seen.size > 2000) continue;
      seen.add(node);
      for (const k of Object.keys(node)) {
        try {
          const v = node[k];
          if (v && typeof v === 'object') {
            if (v.commands && typeof v.commands.insertContent === 'function') return v;
            if (v.editor && v.editor.commands) return v.editor;
            queue.push(v);
          }
        } catch(e) {}
      }
      for (const k of Object.getOwnPropertyNames(node)) {
        if (k.startsWith('__reactFiber') || k.startsWith('__reactInternalInstance')) {
          try {
            const fiber = node[k];
            if (fiber) {
              queue.push(fiber);
              if (fiber.memoizedProps) queue.push(fiber.memoizedProps);
              if (fiber.memoizedState) queue.push(fiber.memoizedState);
            }
          } catch(e) {}
        }
      }
    }
    return null;
  };

  // 方法1: 通过 React fiber 找 ProseMirror view.dispatch
  try {
    const view = findView([el, el.parentElement, document.body]);
    if (view && view.state) {
      view.dispatch(view.state.tr.insertText(text));
      if (verify()) return 'ok-pm-fiber';
    }
  } catch(e) {}

  // 方法2: 通过 React fiber 找 tiptap editor
  try {
    const editor = findEditor([el, el.parentElement, document.body]);
    if (editor && editor.commands) {
      editor.commands.insertContent(text);
      if (verify()) return 'ok-tiptap-fiber';
    }
  } catch(e) {}

  // 方法3: beforeinput 事件（不修改 DOM，让 ProseMirror 自己处理插入）
  try {
    el.dispatchEvent(new InputEvent('beforeinput', {
      inputType: 'insertText', data: text,
      bubbles: true, cancelable: true
    }));
    if (verify()) return 'ok-beforeinput';
  } catch(e) {}

  // 方法4: clipboard paste 事件
  try {
    const dt = new DataTransfer();
    dt.setData('text/plain', text);
    el.dispatchEvent(new ClipboardEvent('paste', {
      clipboardData: dt, bubbles: true, cancelable: true
    }));
    if (verify()) return 'ok-paste';
  } catch(e) {}

  // 方法5: 原始方式找 ProseMirror view（pmViewDesc）
  try {
    let view = null;
    if (el.pmViewDesc && el.pmViewDesc.view) view = el.pmViewDesc.view;
    if (view && view.state) {
      view.dispatch(view.state.tr.insertText(text));
      if (verify()) return 'ok-pm';
    }
  } catch(e) {}

  // 方法6: execCommand / textarea（React 需用 native setter + InputEvent）
  if (el.isContentEditable) {
    try {
      if (document.execCommand('insertText', false, text) && verify()) return 'ok-exec';
    } catch (e) {}
  } else {
    // 直接赋值优先：obscura 的 native setter 可能静默失效（不抛异常但不生效），
    // 而直接赋值已验证可触发 React 状态更新；直接赋值无效时再回退 native setter
    el.value = text;
    if (!(el.value || '').includes(text)) {
      try {
        const proto = el.tagName === 'TEXTAREA'
          ? window.HTMLTextAreaElement.prototype
          : window.HTMLInputElement.prototype;
        const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
        setter && setter.call(el, text);
      } catch (e) {}
    }
    // 用 InputEvent 而非 Event，React 对 input 事件更敏感
    try {
      el.dispatchEvent(new InputEvent('beforeinput', {
        bubbles: true, cancelable: true, inputType: 'insertText', data: text
      }));
    } catch(e) {}
    el.dispatchEvent(new InputEvent('input', {
      bubbles: true, inputType: 'insertText', data: text
    }));
    el.dispatchEvent(new Event('change', {bubbles: true}));
    if (verify()) return 'ok-textarea';

    // React 可能仍未更新状态，尝试通过 fiber 直接调用 onChange
    try {
      for (const k of Object.getOwnPropertyNames(el)) {
        if (k.startsWith('__reactFiber') || k.startsWith('__reactInternalInstance')) {
          const fiber = el[k];
          if (fiber && fiber.memoizedProps) {
            const props = fiber.memoizedProps;
            // React onChange handler 接收 SyntheticEvent
            const handler = props.onChange || props.onInput || props.onKeyDown;
            if (typeof handler === 'function') {
              const synthEvent = {
                target: el, currentTarget: el,
                type: 'change', bubbles: true,
                preventDefault: () => {}, stopPropagation: () => {},
                nativeEvent: new Event('change', {bubbles: true}),
              };
              handler(synthEvent);
              if (verify()) return 'ok-react-onchange';
            }
          }
        }
      }
    } catch(e) {}
  }

  // 方法7: DOM 手动插入（最后兜底，ProseMirror 状态可能不同步）
  try {
    const p = el.querySelector('p') || el;
    p.innerHTML = '';
    const node = document.createTextNode(text);
    p.appendChild(node);
    el.dispatchEvent(new InputEvent('input', {
      bubbles: true, inputType: 'insertText', data: text
    }));
    el.dispatchEvent(new Event('change', {bubbles: true}));
  } catch(e) {}

  return verify() ? 'ok-dom' : 'verify-failed';
})"#;

/// 构造完整的多策略填入 JS（文本 JSON 编码后作为 IIFE 实参）
pub(crate) fn build_fill_js(text: &str) -> anyhow::Result<String> {
    let json = serde_json::to_string(text)?;
    Ok(format!("{FILL_JS_BODY}({json})"))
}
