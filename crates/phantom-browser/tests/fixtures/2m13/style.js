var target = document.getElementById('target');
var style = target.style;
style.color = '#1565c0';
style.fontWeight = '700';
var directStatus = document.getElementById('direct-status');
if (style.color === '#1565c0' && style.fontWeight === '700') { directStatus.textContent = 'Direct style blue'; }
style.setProperty('background-color', '#eeeeee');
var removed = style.removeProperty('background-color');
var methodStatus = document.getElementById('method-status');
if (removed === '#eeeeee' && style.getPropertyValue('background-color') === '') { methodStatus.textContent = 'Property methods stable'; }
style.cssText = 'color: #1565c0; font-weight: 700';
var sameStyle = style === target.style;
var textStatus = document.getElementById('text-status');
if (sameStyle === true && style.cssText === 'color: #1565c0; font-weight: 700') { textStatus.textContent = 'cssText live stable'; }
var finalNode = document.getElementById('final-status');
finalNode.textContent = 'DOM style GREEN';
finalNode.style.color = '#2e7d32';
finalNode.style.fontWeight = '700';
