var field = document.getElementById('field');
var button = document.getElementById('go');
var link = document.getElementById('link');
var form = document.getElementById('form');
var status = document.getElementById('status');

field.value = 'beta';
field.name = 'q';
field.placeholder = 'Search';
field.setAttribute('type', 'email');
field.required = true;
field.readOnly = true;
field.autoFocus = true;

link.href = '/next';
link.target = '_blank';
link.rel = 'noopener';

form.action = '/submit';
form.method = 'post';
form.enctype = 'multipart/form-data';

button.addEventListener('click', function(event) {
  status.textContent = 'Synthetic click target ' + event.target.id;
  status.className = 'ready';
  button.dataset.state = 'clicked';
});
button.click();

document.getElementById('props').textContent = 'Input reflection ' + field.value + ' ' + field.type + ' ' + field.required + ' ' + field.readOnly + ' ' + field.autoFocus;
document.getElementById('link-state').textContent = 'Link reflection ' + link.href + ' ' + link.target + ' ' + link.rel;
document.getElementById('form-state').textContent = 'Form reflection ' + form.action + ' ' + form.method + ' ' + form.enctype;
document.getElementById('final').textContent = 'HTML element interaction GREEN';
