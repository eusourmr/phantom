var target = document.getElementById('target');
target.classList.add('base', 'active');
target.classList.remove('base');
var addStatus = document.getElementById('add-status');
if (target.className === 'active') { addStatus.textContent = 'Class add remove active'; }
var firstToggle = target.classList.toggle('active', false);
var secondToggle = target.classList.toggle('painted', true);
var toggleStatus = document.getElementById('toggle-status');
if (firstToggle === false && secondToggle === true && target.className === 'painted') { toggleStatus.textContent = 'Toggle force stable'; }
var containsStatus = document.getElementById('contains-status');
if (target.classList.contains('painted') === true) { containsStatus.textContent = 'Contains stable'; }
var finalNode = document.getElementById('final-status');
finalNode.textContent = 'DOM classList GREEN';
finalNode.classList.add('painted');
