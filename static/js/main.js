let savedScroll = 0;
let loupeElement = undefined;
let opacityTransitionInProgress = false;
let opacityTransitionTimeout = undefined;
let slideshowIntervalTimer = undefined;

function openLoupe(gridItem) {
    savedScroll = window.pageYOffset;
    setLoupePhoto(gridItem);
    $('.container').addClass('show-loupe');
    $('.loupe .loading').removeClass('hidden');
    window.scrollTo(0, 0);
}

function setLoupePhoto(gridItem) {
    loadPhoto(gridItem, function(gridItem) {
        window.location.hash = $(gridItem).data('uid');
        $('.loupe-photo-index').children('span').text($(gridItem).data('index') + " / " + $('.grid-item').last().data('index'));
        loupeElement = $(gridItem).children('.photo');
        let photo = $('.loupe .photo-large');
        let loadNext = function() {
            opacityTransitionInProgress = false;
            if (opacityTransitionTimeout) {
                clearTimeout(opacityTransitionTimeout);
                opacityTransitionTimeout = undefined;
            }
            photo.attr('src', '');
            $('.loupe .loading').removeClass('hidden');
            photo.one('load', function() {
                $('.loupe .loading').addClass('hidden');
                photo.removeClass('transparent');
            });
            photo.attr('src', $(loupeElement).data('src-large'));
            $('.loupe').css('background-color', $(loupeElement).data('color') + 'FC');
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
            $('.loupe-action-download').off('click');
            $('.loupe-action-download').on('click', function(event) {
                event.preventDefault();
                event.stopPropagation();
                window.open($(loupeElement).data('src-download'));
            });
            if ($('.loupe-metadata').length > 0) {
                const properties = ['title', 'date', 'place', 'camera', 'lens', 'focal-length', 'aperture', 'exposure-time', 'sensitivity'];
                let showInfoButton = false;
                let showGear = false;
                let showSettings = false;
                properties.forEach(function(property) {
                    let infoElement = $('.loupe-metadata-' + property);
                    infoElement.text('');
                    let value = $(loupeElement).data(property);
                    if (typeof(value) == 'string') {
                        value = value.trim();
                    }
                    if (value) {
                        showInfoButton = true;
                    }
                    if (property == 'camera') {
                        if (value) {
                            infoElement.text(value);
                            showGear = true;
                        }
                    } else if (property == 'lens') {
                        if (value) {
                            infoElement.text(value);
                            showGear = true;
                        }
                    } else if (property == 'focal-length') {
                        if (value) {
                            infoElement.text(value + "mm");
                            showSettings = true;
                        }
                    } else if (property == 'aperture') {
                        if (value) {
                            infoElement.text("f/" + value);
                            showSettings = true;
                        }
                    } else if (property == 'exposure-time') {
                        if (value) {
                            infoElement.text(value + "s");
                            showSettings = true;
                        }
                    } else if (property == 'sensitivity') {
                        if (value) {
                            infoElement.text("ISO " + value);
                            showSettings = true;
                        }
                    } else {
                        if (value) {
                            infoElement.text(value);
                            infoElement.parent().removeClass('hidden');
                        } else {
                            infoElement.parent().addClass('hidden');
                        }
                    }
                });
                if (showInfoButton) {
                    $('.loupe-action-info').removeClass('hidden');
                } else {
                    $('.loupe-action-info').addClass('hidden');
                }
                if (showGear) {
                    $('.loupe-metadata-gear').removeClass('hidden');
                } else {
                    $('.loupe-metadata-gear').addClass('hidden');
                }
                if (showSettings) {
                    $('.loupe-metadata-settings').removeClass('hidden');
                } else {
                    $('.loupe-metadata-settings').addClass('hidden');
                }
            }
        };
        if (!opacityTransitionInProgress) {
            if (photo.hasClass('transparent')) {
                loadNext();
            } else {
                photo.one('transitionend', function() {
                    if (event.propertyName == 'opacity' && photo.hasClass('transparent')) {
                        loadNext();
                    }
                });
                opacityTransitionInProgress = true;
                if (opacityTransitionTimeout) {
                    clearTimeout(opacityTransitionTimeout);
                }
                opacityTransitionTimeout = setTimeout(function() {
                    loadNext();
                }, 800);
                photo.addClass('transparent');
            }
        }
    });
}

function closeLoupe() {
    stopSlideshow();
    window.location.hash = '';
    $('.container').removeClass('show-loupe');
    window.scrollTo(0, savedScroll);
    let selected = $('.grid-item.selected');
    if (selected.length > 0) {
        scrollToPhoto(selected);
    }
}

function isLoupeOpen() {
    return $('.container').hasClass('show-loupe');
}

function loupePrev() {
    let prev = $(loupeElement).parent().prev();
    if (prev.length > 0) {
        setLoupePhoto(prev);
        selectPhoto(prev);
    }
}

function loupeNext(loop=false) {
    let next = $(loupeElement).parent().next();
    if (next.length > 0) {
        setLoupePhoto(next);
        selectPhoto(next);
    } else if (loop) {
        loupeFirst();
    }
}

function loupeFirst() {
    let first = $(loupeElement).parents('.grid').children().first();
    setLoupePhoto(first);
    selectPhoto(first);
}

function loupeLast() {
    let last = $(loupeElement).parents('.grid').children().last();
    setLoupePhoto(last);
    selectPhoto(last);
}

function startSlideshow() {
    if (!slideshowIntervalTimer) {
        $('.loupe-action-slideshow-start').addClass('hidden');
        $('.loupe-action-slideshow-stop').removeClass('hidden');
        slideshowIntervalTimer = setInterval(function() {
            loupeNext(true);
        }, slideshowDelay);
    }
}

function stopSlideshow() {
    if (slideshowIntervalTimer) {
        $('.loupe-action-slideshow-start').removeClass('hidden');
        $('.loupe-action-slideshow-stop').addClass('hidden');
        clearInterval(slideshowIntervalTimer);
        slideshowIntervalTimer = undefined;
    }
}

function isSlideshowStarted() {
    return slideshowIntervalTimer != undefined;
}

function scrollToPhoto(element) {
    const margin = 30;
    let viewportTop = window.scrollY;
    let viewportBottom = viewportTop + $(window).height();
    let elementTop = $(element).offset().top;
    let elementBottom = elementTop + $(element).outerHeight();
    if (elementTop - margin < viewportTop) {
        window.scrollBy(0, elementTop - margin - viewportTop);
    } else if (elementBottom + margin > viewportBottom) {
        window.scrollBy(0, elementBottom + margin - viewportBottom);
    }
}

function selectPhoto(gridItem) {
    $('.grid-item.selected').removeClass('selected');
    $(gridItem).addClass('selected');
}

function selectPrev() {
    let selected = $('.grid-item.selected');
    if (selected.length == 0) {
        $('.grid-item').last().addClass('selected');
    } else {
        let prev = selected.prev();
        if (prev.length > 0) {
            selected.removeClass('selected');
            prev.addClass('selected');
            scrollToPhoto(prev);
        }
    }
}

function selectNext() {
    let selected = $('.grid-item.selected');
    if (selected.length == 0) {
        $('.grid-item').first().addClass('selected');
    } else {
        let next = selected.next();
        if (next.length > 0) {
            selected.removeClass('selected');
            next.addClass('selected');
            scrollToPhoto(next);
        }
    }
}

function selectBelow() {
    let selected = $('.grid-item.selected');
    if (selected.length == 0) {
        $('.grid-item').first().addClass('selected');
    } else {
        selectRow(false);
    }
}

function selectAbove() {
    let selected = $('.grid-item.selected');
    if (selected.length == 0) {
        $('.grid-item').first().addClass('selected');
    } else {
        selectRow(true);
    }
}

function selectRow(above) {
    let selected = $('.grid-item.selected');
    let selectedY = selected.offset().top;
    let nextRowY = -1;
    let nextRow = [];
    let firstIndex = $('.grid-item').first().data('index');
    let lastIndex = $('.grid-item').last().data('index');
    for (index = selected.data('index') + (above ? -1 : 1); above && index >= firstIndex || !above && index <= lastIndex; (above ? index-- : index++)) {
        let element = $('[data-index="' + index + '"]');
        let y = element.offset().top;
        if (y != selectedY) {
            if (nextRowY == -1) {
                nextRowY = y;
            } else if (y != nextRowY) {
                break;
            }
            nextRow.push(element);
        }
    }
    if (nextRow.length > 0) {
        if (above) {
            nextRow.reverse();
        }
        let selectedCenterX = selected.offset().left + selected.outerWidth() / 2;
        let previousDistance = -1;
        let nextIndex = -1;
        $(nextRow).each(function() {
            let centerX = $(this).offset().left + $(this).outerWidth() / 2;
            let distance = Math.abs(centerX - selectedCenterX);
            if (previousDistance >= 0 && distance > previousDistance) {
                nextIndex = $(this).data('index') - 1;
                return false;
            }
            previousDistance = distance;
        });
        if (nextIndex == -1) {
            nextIndex = $(nextRow).last().data('index');
        }
        let next = $('[data-index="' + nextIndex + '"]');
        selected.removeClass('selected');
        next.addClass('selected');
        scrollToPhoto(next);
    }
}

function selectFirst() {
    $('.grid-item.selected').removeClass('selected');
    $('.grid-item').first().each(function() {
        $(this).addClass('selected');
        scrollToPhoto(this);
    });
}

function selectLast() {
    $('.grid-item.selected').removeClass('selected');
    $('.grid-item').last().each(function() {
        $(this).addClass('selected');
        scrollToPhoto(this);
    });
}

function deselect() {
    $('.grid-item.selected').removeClass('selected');
}

function loadPhoto(gridItem, callback) {
    if (!$(gridItem).data('loaded')) {
        let request = new XMLHttpRequest();
        request.onreadystatechange = function() {
            if (this.status == 200) {
                if (this.readyState == 4) {
                    $(gridItem).html(request.responseText);
                    let image = $(gridItem).children('.photo');
                    $(image).on('load', function() {
                        $(image).removeClass('transparent');
                        $(image).on('click', function(event) {
                            let openGridItem = $(this).parents('.grid-item');
                            selectPhoto(openGridItem);
                            openLoupe(openGridItem);
                        });
                        if (callback != undefined) {
                            callback(gridItem);
                        }
                    });
                    $(image).on('mouseenter', function(event) {
                        selectPhoto($(event.target).parents('.grid-item'));
                    });
                    $(image).attr('src', $(image).data('src-thumbnail'));
                    $(gridItem).data('loaded', true);
                }
            }
        };
        request.open('GET', $(gridItem).data('load-url'), true);
        request.send();
    } else {
        if (callback != undefined) {
            callback(gridItem);
        }
    }
}

function gridZoomIn() {
    rowHeight *= 1 + rowHeightStep / 100.;
    document.documentElement.style.setProperty('--row-height', rowHeight + 'vh');
}

function gridZoomOut() {
    rowHeight /= 1 + rowHeightStep / 100.;
    document.documentElement.style.setProperty('--row-height', rowHeight + 'vh');
}

$(function() {
    let observer = new IntersectionObserver(function(elements) {
        $(elements).each(function() {
            if (this.isIntersecting) {
                loadPhoto(this.target);
            }
        });
    }, {
        threshold: 0
    });
    $('.grid-item').each(function() {
        observer.observe(this);
    });

    if (window.location.hash) {
        let uid = window.location.hash.substr(1);
        let gridItem = $('[data-uid="' + uid + '"');
        if (gridItem.length > 0) {
            selectPhoto(gridItem);
            openLoupe(gridItem);
        }
    }

    $('.loupe').on('click', function(event) {
        closeLoupe();
        event.preventDefault();
        event.stopPropagation();
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
    $('.loupe-action-info').on('click', function(event) {
        $('.loupe-metadata').toggleClass('invisible');
        event.preventDefault();
        event.stopPropagation();
    });
    $('.loupe-action-slideshow-start').on('click', function(event) {
        startSlideshow();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.loupe-action-slideshow-stop').on('click', function(event) {
        stopSlideshow();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.grid-action-zoom-in').on('click', function(event) {
        gridZoomIn();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.grid-action-zoom-out').on('click', function(event) {
        gridZoomOut();
        event.preventDefault();
        event.stopPropagation();
    });

    window.onkeydown = function(event) {
        if (event.code == 'Escape') {
            event.preventDefault();
            if (isLoupeOpen()) {
                closeLoupe();
            } else {
                deselect();
            }
        } else if (event.code == 'ArrowLeft') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupePrev();
            } else {
                selectPrev();
            }
        } else if (event.code == 'ArrowRight') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else {
                selectNext();
            }
        } else if (event.code == 'ArrowDown') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else {
                selectBelow();
            }
        } else if (event.code == 'ArrowUp') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupePrev();
            } else {
                selectAbove();
            }
        } else if (event.code == 'Home') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeFirst();
            } else {
                selectFirst();
            }
        } else if (event.code == 'End') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeLast();
            } else {
                selectLast();
            }
        } else if (event.code == 'Enter' || event.code == 'KeyF') {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else {
                let selected = $('.grid-item.selected');
                if (selected.length == 0) {
                    selectFirst();
                }
                openLoupe($('.grid-item.selected'));
            }
        } else if (event.code == 'Space') {
            event.preventDefault();
            if (isLoupeOpen()) {
                if (isSlideshowStarted()) {
                    stopSlideshow();
                } else {
                    startSlideshow();
                }
            } else {
                let selected = $('.grid-item.selected');
                if (selected.length == 0) {
                    selectFirst();
                }
                openLoupe($('.grid-item.selected'));
                startSlideshow();
            }
        } else if (event.key == "+") {
            event.preventDefault();
            gridZoomIn();
        } else if (event.key == "-") {
            event.preventDefault();
            gridZoomOut();
        }
    };
});