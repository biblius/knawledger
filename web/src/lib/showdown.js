import showdown from "showdown";

const anchor = '$1<a id="user-content-$3" class="anchor" href="#$3" aria-hidden="true"><svg aria-hidden="true" class="octicon octicon-link" height="16" version="1.1" viewBox="0 0 16 16" width="16"><path fill-rule="evenodd" d="M4 9h1v1H4c-1.5 0-3-1.69-3-3.5S2.55 3 4 3h4c1.45 0 3 1.69 3 3.5 0 1.41-.91 2.72-2 3.25V8.59c.58-.45 1-1.27 1-2.09C10 5.22 8.98 4 8 4H4c-.98 0-2 1.22-2 2.5S3 9 4 9zm9-3h-1v1h1c1 0 2 1.22 2 2.5S13.98 12 13 12H9c-.98 0-2-1.22-2-2.5 0-.83.42-1.64 1-2.09V6.25c-1.09.53-2 1.84-2 3.25C6 11.31 7.55 13 9 13h4c1.45 0 3-1.69 3-3.5S14.5 6 13 6z"></path></svg></a>$4';

showdown.extension('header-anchors', function() {
  return [{
    type: 'html',
    regex: /(<h([1-3]) id="([^"]+?)">)(.*<\/h\2>)/g,
    replace: anchor,
  }];
});

showdown.extension('copy-code', function() {
  return [{
      type: "output",
      filter: function (text) {
        const html = text.replace(
            /<pre.*><code/g,
          `<button class="copy-code-button">&#128203;</button>$&`
        );

        if (typeof window === "undefined") {
          return html;
        }

        if (window.copyCodeListener === true) {
          return html;
        }

        window.addEventListener("click", (e) => {
          const target = e.target;
          if (target?.classList?.contains("copy-code-button") && target.nextElementSibling?.tagName === "PRE") {
            navigator.clipboard.writeText(
              target.nextElementSibling.innerText
            );
          }
        });

        window.copyCodeListener = true;

        return html;
      }
  }];
})

const converter = new showdown.Converter({
  ghCodeBlocks: true,
  ghCompatibleHeaderId: true,
  extensions: ['header-anchors', 'copy-code'],
  tables: true
});

export default converter;
