var allowed = document.getElementById('allowed');
var blocked = document.getElementById('blocked');
var submit = document.getElementById('submit');
var plain = document.getElementById('plain');
var status = document.getElementById('status');

allowed.addEventListener('click', function(event) {
    status.textContent = 'Trusted link ' + event.isTrusted;
    status.className = 'allowed';
    allowed.href = '/mutated-target.html';
});

blocked.addEventListener('click', function(event) {
    status.textContent = 'Blocked link ' + event.isTrusted;
    status.className = 'blocked';
    event.preventDefault();
});

submit.addEventListener('click', function(event) {
    status.textContent = 'Blocked submit ' + event.isTrusted;
    status.className = 'blocked';
    event.preventDefault();
});

plain.addEventListener('click', function(event) {
    status.textContent = 'Plain trusted ' + event.isTrusted;
    status.className = 'plain';
});
