var target = document.getElementById('event-target');
var order = '';

var first = function(event) {
  order = order + 'first-';
  if (this === target) {
    document.getElementById('this-status').textContent = 'Listener this bound';
  }
  if (event.type === 'phantom-ready' && event.target === target && event.currentTarget === target) {
    document.getElementById('event-status').textContent = 'Event object bound';
  }
};

var capture = function(event) {
  order = order + 'capture-';
};

var removed = function(event) {
  order = order + 'removed-';
};

var last = function(event) {
  order = order + 'last';
  document.getElementById('order-status').textContent = order;
  document.getElementById('remove-status').textContent = 'Listener removed';
};

target.addEventListener('phantom-ready', first);
target.addEventListener('phantom-ready', capture, true);
target.addEventListener('phantom-ready', first);
target.addEventListener('phantom-ready', removed);
target.removeEventListener('phantom-ready', removed);
target.addEventListener('phantom-ready', last);

var event = { type: 'phantom-ready' };
var dispatched = target.dispatchEvent(event);
if (dispatched && event.currentTarget === null && order === 'capture-first-last') {
  document.getElementById('final-status').textContent = 'Event target GREEN';
}
