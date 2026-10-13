var list = Array.of(2, 3);
list.unshift(1);
var first = list.shift();
var result = list.concat([4, 5], 6);
result.reverse();

var obj = Object.fromEntries([['alpha', 1], ['beta', 2]]);

document.getElementById('array').textContent =
  'Array final ' + result.join('-') + ' tail ' + result.at(-1) + ' first ' + first;
document.getElementById('object').textContent =
  'Object final ' + Object.keys(obj).join(',') + ' ' + obj.beta;
document.getElementById('string').textContent =
  'String final ' + 'phantom'.substring(0, 4).toUpperCase() + ' ' +
  '7'.padStart(3, '0') + ' ' + 'a-a'.replaceAll('a', 'x');
document.getElementById('number').textContent =
  'Numeric final ' + Math.trunc(3.9) + ' ' + Math.sign(-2) + ' ' + Number.isInteger(3);
document.getElementById('gate').className = 'active';
document.getElementById('done').textContent = 'Web core library FINAL GREEN';
