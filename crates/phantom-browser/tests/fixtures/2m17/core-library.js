console.log('Phantom 2M17 core library');
var target = document.getElementById('target');
var data = JSON.parse('{"name":"Phantom","scores":[2,4]}');
data.scores.push(6);
var arrayState = data.scores.slice(0, 3).join('-');
var text = '  phantom browser  '.trim().toUpperCase();
var keys = Object.keys(data).join(',');
document.getElementById('array').textContent = 'Array state ' + arrayState;
document.getElementById('string').textContent = 'String state ' + text.slice(0, 7);
document.getElementById('json').textContent = 'JSON keys ' + keys;
if (data.scores.includes(6) && text.startsWith('PHANTOM') && JSON.stringify(data).includes('scores')) {
  target.className = 'ok';
  target.textContent = 'Core library active';
  document.getElementById('final').className = 'final';
  document.getElementById('final').textContent = 'Web core library GREEN';
}
