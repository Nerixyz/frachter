import {create} from 'qrcode/lib/core/qrcode';
import {render} from 'qrcode/lib/renderer/svg-tag';

/**
 * @param {string} url
 * @returns {string}
 */
export function renderToSvg(url) {
    const data = create(url);
    return render(data);
}