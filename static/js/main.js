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
    var photo = $('.loupe .photo-large');
    photo.addClass('transparent');
    photo.attr('src', '');
    photo.one('load', function() {
        photo.removeClass('transparent');
    });
    photo.attr('src', $(loupeElement).data('src-full'));
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

$(function() {
    var observer = new IntersectionObserver(function(elements) {
        $(elements).each(function() {
            if (this.isIntersecting) {
                var grid_item = $(this.target);
                if (!grid_item.data('loaded')) {
                    var request = new XMLHttpRequest();
                    request.onreadystatechange = function() {
                        if (this.status == 200) {
                            if (this.readyState == 4) {
                                grid_item.html(request.responseText);
                                var image = grid_item.children('.photo');
                                $(image).on('load', function() {
                                    $(image).removeClass('transparent');
                                    $(image).on('click', function(event) {
                                        openLoupe(this);
                                    });
                                });
                                $(image).attr('src', $(image).data('src-thumbnail'));
                                grid_item.data('loaded', true);
                            }
                        }
                    };
                    request.open('GET', grid_item.data('load-url'), true);
                    request.send();
                }
            }
        });
    }, {
        threshold: 0
    });
    $('.grid-item').each(function() {
        observer.observe(this);
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