/** {@type {HTMLInputElement}} */
const fileInput = document.getElementById('file');
/** {@type {HTMLFormElement}} */
const fileForm = document.getElementById('send-file');
/** {@type {HTMLInputElement}} */
const filename = document.getElementById('filename');
/** {@type {HTMLFormElement}} */
const setTokenForm = document.getElementById('set-token');
/** {@type {HTMLInputElement}} */
const setTokenInput = document.getElementById('token');

function updateFileName() {
    if (fileInput.files.length > 0) {
        filename.textContent = fileInput.files[0].name;
    }
}

function updateToken() {
    if (localStorage.getItem('frachter-token')) {
        fileForm.classList.remove('hidden');
        setTokenForm.classList.add('hidden');
    } else {
        fileForm.classList.add('hidden');
        setTokenForm.classList.remove('hidden');
    }
}

function makeHeaders(other) {
    return {
        'x-frachter-token': localStorage.getItem('frachter-token'),
        ...other,
    }
}

fileInput.addEventListener('input', updateFileName);


fileForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (fileInput.files.length <= 0) return;

    await sendFile(fileInput.files[0]);
    //setTimeout(() => ov.remove(), 2000);
});

setTokenForm.addEventListener('submit', (e) => {
    e.preventDefault();
    if (!setTokenInput.value) return;
    localStorage.setItem('frachter-token', setTokenInput.value);
    updateToken();
});

updateToken();
updateFileName();

function createOverlay({title, content}) {
    const overlay = document.createElement('div');
    overlay.classList.add('overlay');
    document.body.append(overlay);

    const view = document.createElement('div');
    view.classList.add('overlay-view');
    overlay.append(view);

    const titleEl = document.createElement('h1');
    titleEl.textContent = title;
    view.append(titleEl);

    function appendContent(onto, content) {
        if (!content) {
            content = document.createElement('div');
        }
        content.classList.add('overlay-content');
        onto.append(content);

        return content;
    }

    content = appendContent(view, content);

    return {
        overlay,
        view,
        titleEl,
        content,
        update({title, content}) {
            this.titleEl.textContent = title;
            this.content.remove();
            this.content = appendContent(this.view, content);
        },
        remove() {
            const anim = this.overlay.animate({
                opacity: ['1', '0'],
                'backdrop-filter': ['blur(20px)', 'blur(0)']
            }, {duration: 200});
            anim.addEventListener('finish', () => this.overlay.remove());
            anim.play();
        }
    }
}

function createLoader() {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.classList.add('loader');
    svg.setAttribute('viewBox', '-1 -1 12 12');
    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    circle.setAttribute('cx', '5');
    circle.setAttribute('cy', '5');
    circle.setAttribute('r', '5');
    svg.append(circle);
    return svg;
}

function createError(text, cb) {
    const wrap = document.createElement('div');
    wrap.classList.add('error');

    const textEl = document.createElement('h4');
    textEl.textContent = text;
    wrap.append(textEl);

    const exit = document.createElement('button');
    exit.textContent = 'Ok';
    exit.addEventListener('click', cb);
    requestAnimationFrame(() => exit.focus());
    wrap.append(exit);

    return wrap;
}

async function createWaiting(url) {
    const wrap = document.createElement('div');
    wrap.classList.add('waiting');

    const {renderToSvg} = await import('./qr.min.js');
    const svg = renderToSvg(url);
    const qr = document.createElement('div');
    qr.innerHTML = svg;
    qr.classList.add('qrcode');
    wrap.append(qr);

    const urlEl = document.createElement('a');
    urlEl.href = url;
    urlEl.target = '_blank';
    urlEl.textContent = url;
    wrap.append(urlEl);

    return wrap;
}

function createTransferring() {
    const wrap = document.createElement('div');
    wrap.classList.add('transferring');

    const progress = document.createElement('div');
    progress.classList.add('progress');
    wrap.append(progress);

    const text = document.createElement('p');
    text.textContent = '0%';
    wrap.append(text);

    return [wrap, n => {
        n = Math.max(0, Math.min(1, n)) * 100;
        progress.style.setProperty('--progress', `${n}%`);
        text.textContent = `${n.toFixed(0)}%`;
    }];
}

/**
 *
 * @param {File} file
 * @returns {Promise<void>}
 */
async function sendFile(file) {
    const overlay = createOverlay({title: 'Creating Transfer...', content: createLoader()});
    try {
        const res = await fetch('/api/transfers', {
            method: 'PUT',
            body: JSON.stringify({
                filename: file.name,
                contentType: file.type,
            }),
            headers: makeHeaders({'content-type': 'application/json'}),
        });
        const json = await tryJson(res);

        const recvUrl = `${location.origin}/api/receive/${json.id}`;
        overlay.update({title: 'Waiting for peer...', content: await createWaiting(recvUrl)});
        await waitForPeer();
        const [content, progressCb] = createTransferring();
        overlay.update({title: 'Sending...', content});
        await transfer(file, progressCb);
        overlay.remove();
    } catch (e) {
        overlay.update({title: 'Error', content: createError(e.toString(), () => overlay.remove())})
    }
}

async function waitForPeer() {
    while (true) {
        const res = await fetch('/api/transfer/wait', {
            headers: makeHeaders(),
        });
        if (res.ok) {
            return;
        }
        const json = await res.json();
        if (res.status !== 504) {
            throw new Error(`${res.status} ${res.statusText} - ${json?.message ?? JSON.stringify(json)}`);
        }
    }
}

function transfer(file, progressCb) {
    return new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.addEventListener('progress', ({loaded, total}) => {
            progressCb(loaded / total);
        });
        xhr.addEventListener('load', () => {
            if (xhr.status >= 200 && xhr.status < 300) {
                resolve();
            } else {
                reject(tryJsonErrorXhr(xhr));
            }
        });
        xhr.addEventListener('error', () => {
            reject(new Error("Request failed"));
        });
        xhr.addEventListener('abort', () => {
            reject(new Error("Aborted"));
        });
        xhr.open('POST', '/api/transfer/send');
        xhr.setRequestHeader('x-frachter-token', localStorage.getItem('frachter-token'));
        xhr.send(file);
    });
}

async function tryJson(res) {
    const json = res.headers.get('content-type').startsWith('application/json') ? await res.json() : await res.text();
    if (!res.ok || !json.id) {
        throw new Error(`${res.status} ${res.statusText} - ${typeof json === 'string' ? json : json?.message ?? JSON.stringify(json)}`);
    }
    return json;
}

function tryJsonErrorXhr(xhr) {
    const json = xhr.getResponseHeader('content-type').startsWith('application/json') ? JSON.parse(xhr.responseText) : xhr.responseText;
    return new Error(`${xhr.status} ${xhr.statusText} - ${typeof json === 'string' ? json : json?.message ?? JSON.stringify(json)}`);
}
