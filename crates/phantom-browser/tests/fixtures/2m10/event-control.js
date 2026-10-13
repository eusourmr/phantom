var parent = document.getElementById('parent');
var target = document.getElementById('target');

var cancelEvent = { type: 'cancel', bubbles: true, cancelable: true };
target.addEventListener('cancel', function(event) { event.preventDefault(); });
var cancelResult = target.dispatchEvent(cancelEvent);
var cancel = document.getElementById('cancel');
if (cancelResult === false && cancelEvent.defaultPrevented === true) { cancel.textContent = 'Default prevented dispatch false'; cancel.setAttribute('class', 'pass'); }

var stopLog = '';
target.addEventListener('stop', function(event) { stopLog = stopLog + 'target-a'; event.stopPropagation(); });
target.addEventListener('stop', function(event) { stopLog = stopLog + '-target-b'; });
parent.addEventListener('stop', function(event) { stopLog = stopLog + '-parent'; });
target.dispatchEvent({ type: 'stop', bubbles: true });
var stop = document.getElementById('stop');
if (stopLog === 'target-a-target-b') { stop.textContent = 'Stop propagation same target'; stop.setAttribute('class', 'pass'); }

var immediateLog = '';
target.addEventListener('immediate', function(event) { immediateLog = immediateLog + 'first'; event.stopImmediatePropagation(); });
target.addEventListener('immediate', function(event) { immediateLog = immediateLog + '-second'; });
parent.addEventListener('immediate', function(event) { immediateLog = immediateLog + '-parent'; });
target.dispatchEvent({ type: 'immediate', bubbles: true });
var immediate = document.getElementById('immediate');
if (immediateLog === 'first') { immediate.textContent = 'Stop immediate first only'; immediate.setAttribute('class', 'pass'); }

var onceCount = 0;
var onceListener = function(event) { onceCount = onceCount + 1; };
target.addEventListener('once-only', onceListener, { once: true });
target.dispatchEvent({ type: 'once-only' });
target.dispatchEvent({ type: 'once-only' });
var once = document.getElementById('once');
if (onceCount === 1) { once.textContent = 'Once listener one call'; once.setAttribute('class', 'pass'); }

var finalState = document.getElementById('final');
if (cancelResult === false && cancelEvent.defaultPrevented === true && stopLog === 'target-a-target-b' && immediateLog === 'first' && onceCount === 1) { finalState.textContent = 'Event control GREEN'; finalState.setAttribute('class', 'pass'); }
