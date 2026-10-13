var left = document.getElementById('left');
var right = document.getElementById('right');
var a = document.getElementById('a');
var b = document.getElementById('b');
var c = document.getElementById('c');
var treeStatus = document.getElementById('tree-status');
if (left.childElementCount === 2 && left.children.length === 2 && left.firstElementChild === a && left.lastElementChild === b && a.nextElementSibling === b && b.previousElementSibling === a && b.parentElement === left) {
  treeStatus.textContent = 'Tree navigation stable';
  treeStatus.classList.add('active');
}
right.appendChild(b);
var mutationStatus = document.getElementById('mutation-status');
if (left.childElementCount === 1 && left.lastElementChild === a && right.childElementCount === 2 && right.firstElementChild === c && right.lastElementChild === b && c.nextElementSibling === b && b.previousElementSibling === c && b.parentElement === right) {
  mutationStatus.textContent = 'Append order coherent';
}
b.remove();
var finalStatus = document.getElementById('final-status');
if (right.childElementCount === 1 && right.lastElementChild === c && b.parentElement === null && b.isConnected === false) {
  finalStatus.textContent = 'DOM tree navigation GREEN';
  finalStatus.classList.add('ready');
}
