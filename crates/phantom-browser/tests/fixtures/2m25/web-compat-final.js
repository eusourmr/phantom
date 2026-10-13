var app = document.getElementById('app');
var panel = document.getElementById('panel');
var status = document.getElementById('status');
var styleTarget = document.getElementById('style-target');
var mutationZone = document.getElementById('mutation-zone');
var field = document.getElementById('field');
var items = document.querySelectorAll("#list > .item[data-role='item']");

app.dataset.phase = 'script';
panel.classList.add('active');
panel.title = '2M final';
styleTarget.style.color = '#1565c0';
styleTarget.textContent = 'Style direct GREEN';
field.value = 'beta';
field.type = 'search';
field.required = true;

var created = document.createElement('span');
created.id = 'created';
created.textContent = 'Created';
mutationZone.append(created);
var before = document.createElement('span');
before.id = 'before';
before.textContent = 'Before';
created.before(before);
var replacement = document.createElement('strong');
replacement.id = 'replacement';
replacement.textContent = 'Replacement';
document.getElementById('replace-me').replaceWith(replacement);

var doubled = [1, 2, 3, 4].map(function(value) { return value * 2; }).filter(function(value) { return value > 4; }).join('-');
var keys = Object.keys({a: 1, b: 2}).join(',');
var parsed = JSON.parse('{"name":"Phantom"}').name.toUpperCase();
var holder = { value: 'CTX', read: function() { return this.value; } };
var make = function(prefix) { return function(value) { return prefix + value; }; };
var closure = make('C');
var functionState = holder.read() + '-' + closure('LOSURE');

var syntheticTrusted = true;
var syntheticButton = document.getElementById('synthetic-button');
syntheticButton.addEventListener('click', function(event) { syntheticTrusted = event.isTrusted; });
syntheticButton.click();

var nativePanel = document.getElementById('native-panel');
var continueLink = document.getElementById('continue-link');
var blockedLink = document.getElementById('blocked-link');
var eventStatus = document.getElementById('event-status');
var blockedStatus = document.getElementById('blocked-status');
var nativeTrace = '';
nativePanel.addEventListener('click', function(event) { nativeTrace = nativeTrace + 'C' + event.isTrusted; }, true);
continueLink.addEventListener('click', function(event) {
    nativeTrace = nativeTrace + '-T' + event.isTrusted;
    continueLink.dataset.clicked = 'yes';
    continueLink.href = '/web-compat-final-target.html';
    queueMicrotask(function() {
        eventStatus.textContent = 'Native ' + nativeTrace;
        eventStatus.classList.add('native');
    });
});
nativePanel.addEventListener('click', function(event) { nativeTrace = nativeTrace + '-B' + event.isTrusted; });
blockedLink.addEventListener('click', function(event) {
    blockedStatus.textContent = 'Blocked trusted ' + event.isTrusted;
    blockedStatus.classList.add('blocked');
    event.preventDefault();
});

var asyncTrace = 'sync';
queueMicrotask(function() {
    asyncTrace = asyncTrace + '-micro';
    app.dataset.micro = 'yes';
});
Promise.resolve({ then: function(resolve) { resolve('thenable'); } }).then(function(value) {
    asyncTrace = asyncTrace + '-' + value;
    app.classList.add('promise');
});
setTimeout(function() {
    asyncTrace = asyncTrace + '-timer';
    app.dataset.timer = 'yes';
}, 0);
fetch('/web-compat-final-data.txt').then(function(response) {
    return response.text();
}).then(function(text) {
    asyncTrace = asyncTrace + '-fetch';
    status.textContent = '2M FINAL ASYNC GREEN ' + text +
        ' selector ' + items.length +
        ' builtins ' + doubled + ' ' + keys + ' ' + parsed +
        ' function ' + functionState +
        ' tree ' + replacement.parentElement.id + ' ' + mutationZone.children.length +
        ' synthetic ' + syntheticTrusted +
        ' reflection ' + panel.getAttribute('title') + ' ' + field.value + ' ' + field.type + ' ' + field.required +
        ' async ' + asyncTrace;
    status.classList.add('ready');
});
