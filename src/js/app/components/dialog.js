// ============================================================
// dialog.js
// The "Are you sure?" box.
// ============================================================

import { $ } from '../state.js';

const dialog = $('#confirm');

/** Asks first. Resolves true if the main button was pressed. */
export function confirmAction({ title, text, action }) {
  $('#confirm-title').textContent = title;
  $('#confirm-text').textContent = text;
  $('#confirm-ok').textContent = action;
  dialog.returnValue = '';
  dialog.classList.remove('is-closing');
  dialog.showModal();
  document.activeElement?.blur(); // no pre-focused button ring on open
  return new Promise((resolve) => {
    dialog.addEventListener('close', () => resolve(dialog.returnValue === 'ok'), { once: true });
  });
}

function close(value) {
  if (dialog.classList.contains('is-closing')) return;
  dialog.classList.add('is-closing');
  dialog.addEventListener('animationend', () => dialog.close(value), { once: true });
}

dialog.querySelector('form').addEventListener('submit', (e) => {
  e.preventDefault();
  close(e.submitter?.value ?? '');
});

dialog.addEventListener('cancel', (e) => {
  e.preventDefault();
  close('');
});
