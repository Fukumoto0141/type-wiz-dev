document.addEventListener('DOMContentLoaded', () => {
    const slides = [
        '01.html',
        '02.html',
        '03.html',
        '04.html',
        '05.html'
    ];

    // URLからファイル名を取得。クエリパラメータなどは無視。
    const path = window.location.pathname;
    const currentFile = path.substring(path.lastIndexOf('/') + 1);
    
    const currentIndex = slides.indexOf(currentFile);

    // もしファイル名が取得できない、またはリストにない場合は何もしない
    if (currentIndex === -1) return;

    function goNext() {
        const nextIndex = (currentIndex + 1) % slides.length;
        window.location.href = slides[nextIndex];
    }

    function goPrev() {
        const prevIndex = (currentIndex - 1 + slides.length) % slides.length;
        window.location.href = slides[prevIndex];
    }

    document.addEventListener('keydown', (e) => {
        if (e.key === 'ArrowRight' || e.key === ' ' || e.key === 'Enter') {
            goNext();
        } else if (e.key === 'ArrowLeft') {
            goPrev();
        }
    });

    document.addEventListener('click', (e) => {
        // リンクやボタン、入力フォームなどのクリックは無視
        if (e.target.closest('a') || e.target.closest('button') || e.target.closest('input') || e.target.closest('textarea')) return;
        
        // 画面の左右で判定
        if (e.clientX < window.innerWidth / 2) {
            goPrev();
        } else {
            goNext();
        }
    });
});
