var root = document.getElementById('root');
var parent = document.getElementById('parent');
var target = document.getElementById('target');
var order = '';
var contextOk = true;
var phaseOk = true;

var rootCapture = function(event) {
  order = order + 'root-capture-';
  if (this !== root || event.target !== target || event.currentTarget !== root) { contextOk = false; }
  if (event.eventPhase !== 1) { phaseOk = false; }
};
var parentCapture = function(event) {
  order = order + 'parent-capture-';
  if (this !== parent || event.currentTarget !== parent) { contextOk = false; }
  if (event.eventPhase !== 1) { phaseOk = false; }
};
var targetCapture = function(event) {
  order = order + 'target-capture-';
  if (this !== target || event.currentTarget !== target) { contextOk = false; }
  if (event.eventPhase !== 2) { phaseOk = false; }
};
var targetBubble = function(event) {
  order = order + 'target-bubble-';
  if (event.eventPhase !== 2) { phaseOk = false; }
};
var parentBubble = function(event) {
  order = order + 'parent-bubble-';
  if (event.eventPhase !== 3) { phaseOk = false; }
};
var rootBubble = function(event) {
  order = order + 'root-bubble';
  if (event.eventPhase !== 3) { phaseOk = false; }
};

root.addEventListener('phantom-flow', rootCapture, true);
parent.addEventListener('phantom-flow', parentCapture, true);
target.addEventListener('phantom-flow', targetCapture, true);
target.addEventListener('phantom-flow', targetBubble);
parent.addEventListener('phantom-flow', parentBubble);
root.addEventListener('phantom-flow', rootBubble);

var event = { type: 'phantom-flow', bubbles: true };
var dispatched = target.dispatchEvent(event);
var expected = 'root-capture-parent-capture-target-capture-target-bubble-parent-bubble-root-bubble';
if (order === expected) { document.getElementById('order-status').textContent = 'Capture target bubble order'; }
if (phaseOk && event.eventPhase === 0) { document.getElementById('phase-status').textContent = 'Event phases 1-2-3-reset'; }
if (contextOk && event.target === target && event.currentTarget === null && event.bubbles === true) { document.getElementById('context-status').textContent = 'Propagation context bound'; }
if (dispatched && order === expected && phaseOk && contextOk) { document.getElementById('final-status').textContent = 'Event propagation GREEN'; }
