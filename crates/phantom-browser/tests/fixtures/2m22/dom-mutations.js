var board = document.getElementById('board');
var a = document.getElementById('a');
var b = document.getElementById('b');
var c = document.getElementById('c');
var d = document.getElementById('d');
var e = document.getElementById('e');

board.prepend(d);
b.before(e);
a.after(c);
b.replaceWith(e);
board.replaceChildren(c, d, a);
var removed = board.removeChild(d);
board.prepend(e);
board.append(b);

var statusOne = document.getElementById('status-one');
if (board.children[0].id === 'e' && board.children[1].id === 'c' && board.children[2].id === 'a' && board.children[3].id === 'b') {
  statusOne.textContent = 'Mutation order stable e-c-a-b';
  statusOne.classList.add('ready');
}

var statusTwo = document.getElementById('status-two');
if (removed === d && d.isConnected === false && d.parentElement === null) {
  statusTwo.textContent = 'removeChild identity disconnected stable';
}

e.textContent = 'Mutation selected';
e.classList.add('selected');
var finalNode = document.getElementById('final-status');
finalNode.textContent = 'DOM mutation API II GREEN';
finalNode.classList.add('done');
