var app = document.getElementById('app');
var primary = document.getElementById('primary');
var hero = document.querySelector('#app > section.panel.primary[data-state=ready] article.card.featured > span.label[data-kind=hero][data-label="hero card"]');
var labels = document.querySelectorAll('#app section.panel .label, #app > section.secondary > span.label[data-kind=side]');
var queryStatus = document.getElementById('query-status');
if (hero.id === 'hero' && labels.length === 3 && labels[0].id === 'hero' && labels[1].id === 'plain' && labels[2].id === 'side') {
  queryStatus.textContent = 'Selector list document order stable';
  queryStatus.classList.add('active');
}
var matchStatus = document.getElementById('match-status');
if (hero.matches('span.label[data-kind=hero], div.nope') === true && hero.closest('#app > section.panel.primary, body.nope') === primary) {
  matchStatus.textContent = 'Matches closest compound stable';
}
var scoped = primary.querySelectorAll('article.card > span.label');
var scopeStatus = document.getElementById('scope-status');
if (scoped.length === 2 && scoped[0].id === 'hero' && scoped[1].id === 'plain' && primary.querySelector('#primary') === null && primary.querySelectorAll('*').length === 4) {
  scopeStatus.textContent = 'Element scope descendants stable';
}
hero.classList.add('selected');
hero.textContent = 'Hero selected';
var finalStatus = document.getElementById('final-status');
if (hero.classList.contains('selected') === true && hero.textContent === 'Hero selected') {
  finalStatus.textContent = 'DOM selector API II GREEN';
  finalStatus.classList.add('ready');
}
