var target = document.getElementById('target');

var passiveEvent = { type: 'passive-final', cancelable: true };
target.addEventListener('passive-final', function(event) { event.preventDefault(); }, { passive: true });
var passiveResult = target.dispatchEvent(passiveEvent);
var passiveState = document.getElementById('passive');
if (passiveResult === true && passiveEvent.defaultPrevented === false) { passiveState.textContent = 'Passive default preserved'; passiveState.setAttribute('class', 'pass'); }

var mutationLog = '';
var lateListener = function(event) { mutationLog = mutationLog + 'L'; };
var removedListener = function(event) { mutationLog = mutationLog + 'B'; };
var mutatingListener = function(event) { mutationLog = mutationLog + 'A'; target.addEventListener('mutation-final', lateListener); target.removeEventListener('mutation-final', removedListener); };
target.addEventListener('mutation-final', mutatingListener);
target.addEventListener('mutation-final', removedListener);
target.dispatchEvent({ type: 'mutation-final' });
target.dispatchEvent({ type: 'mutation-final' });
var mutationState = document.getElementById('mutation');
if (mutationLog === 'AAL') { mutationState.textContent = 'Dispatch mutation stable'; mutationState.setAttribute('class', 'pass'); }

var objectCount = 0;
var listenerObject = { handleEvent: function(event) { if (this === listenerObject && event.currentTarget === target) { objectCount = objectCount + 1; } } };
target.addEventListener('object-final', listenerObject);
target.dispatchEvent({ type: 'object-final' });
target.removeEventListener('object-final', listenerObject, { capture: false, passive: true, once: true });
target.dispatchEvent({ type: 'object-final' });
var objectState = document.getElementById('object');
if (objectCount === 1) { objectState.textContent = 'Listener object handled'; objectState.setAttribute('class', 'pass'); }

var duplicateCount = 0;
var duplicateListener = function(event) { duplicateCount = duplicateCount + 1; event.preventDefault(); };
target.addEventListener('duplicate-final', duplicateListener, { passive: true });
target.addEventListener('duplicate-final', duplicateListener, { once: true });
var duplicateFirst = { type: 'duplicate-final', cancelable: true };
var duplicateSecond = { type: 'duplicate-final', cancelable: true };
var duplicateFirstResult = target.dispatchEvent(duplicateFirst);
var duplicateSecondResult = target.dispatchEvent(duplicateSecond);
var duplicateState = document.getElementById('duplicate');
if (duplicateCount === 2 && duplicateFirstResult === true && duplicateSecondResult === true && duplicateFirst.defaultPrevented === false && duplicateSecond.defaultPrevented === false) { duplicateState.textContent = 'Duplicate identity stable'; duplicateState.setAttribute('class', 'pass'); }

var reentrantCount = 0;
var reentrantListener = function(event) { reentrantCount = reentrantCount + 1; target.dispatchEvent({ type: 'reentrant-final' }); };
target.addEventListener('reentrant-final', reentrantListener, { once: true });
target.dispatchEvent({ type: 'reentrant-final' });
var reentrantState = document.getElementById('reentrant');
if (reentrantCount === 1) { reentrantState.textContent = 'Reentrant once stable'; reentrantState.setAttribute('class', 'pass'); }

var finalState = document.getElementById('final');
if (passiveResult === true && passiveEvent.defaultPrevented === false && mutationLog === 'AAL' && objectCount === 1 && duplicateCount === 2 && duplicateFirstResult === true && duplicateSecondResult === true && duplicateFirst.defaultPrevented === false && duplicateSecond.defaultPrevented === false && reentrantCount === 1) { finalState.textContent = 'Event listener FINAL GREEN'; finalState.setAttribute('class', 'pass'); }
