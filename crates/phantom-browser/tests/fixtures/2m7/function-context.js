var makeFormatter = function(prefix) {
  return function(value) {
    return prefix + value;
  };
};

var closureFormat = makeFormatter('Closure ');
document.getElementById('closure-status').textContent = closureFormat('captured');

var methodHolder = {
  label: 'This bound',
  render: function() {
    return this.label;
  }
};
document.getElementById('this-status').textContent = methodHolder.render();

var makeAsyncWriter = function(prefix) {
  return function() {
    document.getElementById('async-status').textContent = prefix + 'timer';
    document.getElementById('final-status').textContent = 'Function context GREEN';
  };
};

setTimeout(makeAsyncWriter('Closure '), 1);
