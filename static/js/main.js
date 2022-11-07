var savedScroll = 0;
var loupeElement = undefined;

function openLoupe(gridElement) {
    savedScroll = window.pageYOffset;
    setLoupeImage(gridElement);
    $('.container').addClass('show-loupe');
    window.scrollTo(0, 0);
}

function closeLoupe() {
    $('.container').removeClass('show-loupe');
    window.scrollTo(0, savedScroll);
}

function isLoupeOpen() {
    return $('.container').hasClass('show-loupe');
}

function loupePrev() {
    let prev = $(loupeElement).parent().prev();
    if (prev.length > 0) {
        setLoupeImage(prev.children('.photo'));
    }
}

function loupeNext() {
    let next = $(loupeElement).parent().next();
    if (next.length > 0) {
        setLoupeImage(next.children('.photo'));
    }
}

function setLoupeImage(element) {
    loupeElement = element;
    $('.photo-large').attr('src', $(loupeElement).data('src'));
    $('.loupe').css('background-color', '#' + $(loupeElement).data('color') + 'FC');
    if ($(loupeElement).parent().prev().length > 0) {
        $('.loupe-prev').removeClass('hidden');
    } else {
        $('.loupe-prev').addClass('hidden');
    }
    if ($(loupeElement).parent().next().length > 0) {
        $('.loupe-next').removeClass('hidden');
    } else {
        $('.loupe-next').addClass('hidden');
    }
}

$(document).ready(function() {
    $('.grid .grid-item .photo').each(function(index, element) {
        $(element).attr('src', $(element).data('thumbnail'));
    });

    $('.grid .grid-item .photo').on('click', function(event) {
        openLoupe(this);
    });
    $('.loupe-photo').on('click', function(event) {
        closeLoupe();
    });
    $('.loupe-prev').on('click', function(event) {
        loupePrev();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.loupe-next').on('click', function(event) {
        loupeNext();
        event.preventDefault();
        event.stopPropagation();
    });

    window.onkeydown = function(event) {
        if (isLoupeOpen()) {
            if (event.key == 'Escape') {
                closeLoupe();
            } else if (event.key == 'ArrowLeft') {
                loupePrev();
            } else if (event.key == 'ArrowRight') {
                loupeNext();
            }
        }
    };
});