console.log('Phantom 2M16 builtins');
var target = document.getElementById('target');
var score = Math.max(Math.round(5.4), Math.floor(6.9));
var parsed = parseInt('42px', 10);
var decimal = parseFloat('3.5rem');
var checks = Array.isArray([1, 2]) && Number.isFinite(score) && isNaN('phantom');
document.getElementById('math').textContent = 'Math score ' + score;
document.getElementById('parse').textContent = 'Parsed 42 and ' + decimal;
document.getElementById('checks').textContent = checks ? 'Builtins checks stable' : 'Builtins checks failed';
if (checks && parsed === 42) {
  target.className = 'ok';
  target.textContent = 'Web globals active';
  document.getElementById('final').className = 'final';
  document.getElementById('final').textContent = 'Web builtins GREEN';
}
