// A small SPA: nothing is server-rendered. Every element is created here,
// after an async fetch and a timer, so a successful render proves the engine
// executes JS, drives timers, and completes in-page network requests.

const app = document.getElementById('app');

function el(tag, text, cls) {
  const node = document.createElement(tag);
  if (text) node.textContent = text;
  if (cls) node.className = cls;
  return node;
}

async function boot() {
  const res = await fetch('data.json');
  const data = await res.json();

  document.title = `${data.project} — ${data.version}`;

  const header = el('header');
  header.appendChild(el('h1', data.project));
  header.appendChild(el('p', `Version ${data.version}`, 'version'));
  app.appendChild(header);

  const list = el('ul', null, 'releases');
  for (const item of data.releases) {
    const li = el('li', `${item.tag} — ${item.date}`);
    li.setAttribute('data-tag', item.tag);
    list.appendChild(li);
  }

  // Deferred mounting: content only appears after a timer fires.
  setTimeout(() => {
    app.appendChild(el('h2', 'Releases'));
    app.appendChild(list);

    const total = el('p', `Total releases: ${data.releases.length}`, 'total');
    total.id = 'total';
    app.appendChild(total);

    window.__spaMounted = true;
  }, 120);
}

boot();
