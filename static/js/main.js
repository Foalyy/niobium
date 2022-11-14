let savedScroll = 0;
let loupeElement = undefined;
let opacityTransitionInProgress = false;
let opacityTransitionTimeout = undefined;
let slideshowIntervalTimer = undefined;
let loadGridBatchSize = 50;
let loadingGrid = false;
let mouseEnterEnabled = true;

let gridStartIntersectionObserver = new IntersectionObserver(function(elements) {
    if (elements[0].isIntersecting) {
        loadGrid(true);
    }
}, {
    threshold: 0.2
});

let gridEndIntersectionObserver = new IntersectionObserver(function(elements) {
    if (elements[0].isIntersecting) {
        loadGrid();
    }
}, {
    threshold: 0.2
});

let gridItemIntersectionObserver = new IntersectionObserver(function(elements) {
    $(elements).each(function() {
        if (this.isIntersecting) {
            loadPhoto(this.target);
        }
    });
}, {
    threshold: 0
});

function loadGrid(before=false, preselectedUID=undefined, around=false) {
    if (loadingGrid) {
        return;
    }
    loadingGrid = true;

    let request = new XMLHttpRequest();
    request.onreadystatechange = function() {
        if (this.status == 200) {
            if (this.readyState == 4) {
                loadingGrid = false;
                $('.grid-loading').addClass('hidden');
                let gridContent = $('.grid-content');
                $(request.responseText.replace(/\n/g, '').trim()).each(function(index, gridItem) {
                    if (gridItem.nodeName == "DIV") {
                        let gridItems = gridContent.children();
                        let inserted = false;
                        for (i = gridItems.length - 1; i >= 0; i--) {
                            let loopGridItem = $(gridItems.get(i));
                            if ($(gridItem).data('index') >= loopGridItem.data('index')) {
                                if ($(gridItem).data('index') != loopGridItem.data('index')) {
                                    $(gridItem).insertAfter(loopGridItem);
                                }
                                inserted = true;
                                break;
                            }
                        }
                        if (!inserted) {
                            $(gridItem).prependTo(gridContent);
                        }
                        gridItemIntersectionObserver.observe(gridItem);
                    }
                });
                if (preselectedUID && !around) {
                    loadGrid(false, preselectedUID, true);
                    let gridItem = $('[data-uid="' + preselectedUID + '"]');
                    selectPhoto(gridItem);
                    openLoupe(gridItem);
                } else {
                    if (!isLoupeOpen()) {
                        setTimeout(function() {
                            connectGridLoaderObservers();
                        }, 500);
                    }
                }
                scrollToSelectedPhoto();
                if (window.scrollY < $('.grid-content').offset().top) {
                    scrollToTop();
                }
            }
        }
    };
    let args = '';
    if (preselectedUID) {
        if (!around) {
            args = '?uid=' + preselectedUID;
        } else {
            let start = $('[data-uid="' + preselectedUID + '"').data('index') - loadGridBatchSize/2;
            if (start < 0) {
                start = 0;
            }
            let count = loadGridBatchSize;
            args = "?start=" + start + "&count=" + count;
        }
    } else {
        let start = 0;
        let count = loadGridBatchSize;
        let gridItems = $('.grid-item');
        if (before) {
            start = gridItems.first().data('index') - count;
            if (start < 0) {
                count += start;
                start = 0;
            }
        } else {
            if (gridItems.length > 0) {
                start = gridItems.last().data('index') + 1;
            }
        }
        args = "?start=" + start + "&count=" + count;
    }
    request.open('GET', loadGridURL + args, true);
    request.send();
    disconnectGridLoaderObservers();
}

function connectGridLoaderObservers() {
    if ($('.grid-item').first().data('index') > 0) {
        $('.grid-content-loading-start').removeClass('hidden');
        gridStartIntersectionObserver.observe($('.grid-content-loading-start')[0]);
    } else {
        $('.grid-content-loading-start').addClass('hidden');
    }
    if ($('.grid-item').last().data('index') < $('.grid-item').last().data('count') - 1) {
        $('.grid-content-loading-end').removeClass('hidden');
        gridEndIntersectionObserver.observe($('.grid-content-loading-end')[0]);
    } else {
        $('.grid-content-loading-end').addClass('hidden');
    }
}

function disconnectGridLoaderObservers() {
    gridStartIntersectionObserver.disconnect();
    gridEndIntersectionObserver.disconnect();
}

function loadPhoto(gridItem, callback) {
    if (!$(gridItem).data('loaded')) {
        let request = new XMLHttpRequest();
        request.onreadystatechange = function() {
            if (this.status == 200) {
                if (this.readyState == 4) {
                    if ($(gridItem).children('img').length > 0) {
                        return;
                    }
                    $(request.responseText.replace(/\n/g, '').trim()).prependTo($(gridItem));
                    let image = $(gridItem).children('.photo');
                    $(image).on('load', function() {
                        $(image).parent().children('.loading').remove()
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
                        if (mouseEnterEnabled) {
                            selectPhoto($(event.target).parents('.grid-item'));
                        }
                    });
                    $(image).on('mouseleave', function(event) {
                        $(event.target).parents('.grid-item').removeClass('selected');
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

function scrollToTop() {
    window.scrollTo(0, $('.grid-content').offset().top);
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

function scrollToSelectedPhoto() {
    let selected = $('.grid-item.selected');
    if (selected.length > 0) {
        scrollToPhoto(selected);
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

function gridZoomIn() {
    rowHeight *= 1 + rowHeightStep / 100.;
    document.documentElement.style.setProperty('--row-height', rowHeight + 'vh');
}

function gridZoomOut() {
    rowHeight /= 1 + rowHeightStep / 100.;
    document.documentElement.style.setProperty('--row-height', rowHeight + 'vh');
}

function openNavigationPanel() {
    $('.navigation-panel-container').removeClass('invisible');
    window.location.hash = 'nav';
}

function closeNavigationPanel() {
    $('.navigation-panel-container').addClass('invisible');
    window.location.hash = '';
}

function openLoupe(gridItem) {
    mouseEnterEnabled = false;
    disconnectGridLoaderObservers();
    savedScroll = window.pageYOffset;
    setLoupePhoto(gridItem);
    $('.container').addClass('show-loupe');
    $('.loupe-loading').removeClass('hidden');
    $('.grid-content-loading-start').addClass('hidden');
    $('.grid-content-loading-end').addClass('hidden');
    scrollToTop();
}

function setLoupePhoto(gridItem) {
    loadPhoto(gridItem, function(gridItem) {
        window.location.hash = $(gridItem).data('uid');
        $('.loupe-photo-index').children('span').text(($(gridItem).data('index') + 1) + " / " + $('.grid-item').first().data('count'));
        loupeElement = $(gridItem).children('.photo');
        let photo = $('.loupe .photo-large');
        let loadNext = function() {
            opacityTransitionInProgress = false;
            if (opacityTransitionTimeout) {
                clearTimeout(opacityTransitionTimeout);
                opacityTransitionTimeout = undefined;
            }
            photo.attr('src', '');
            $('.loupe-loading').removeClass('hidden');
            photo.one('load', function() {
                $('.loupe-loading').addClass('hidden');
                photo.removeClass('transparent');
                if (showMetadata) {
                    $('.loupe-metadata').removeClass('invisible');
                }
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
                downloadCurrentPhoto();
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
            $('.loupe-metadata').addClass('invisible');
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
    let gridItem = $(loupeElement).parent();
    $(gridItem).addClass('selected');
    stopSlideshow();
    window.location.hash = '';
    $('.container').removeClass('show-loupe');
    if ($('.grid-item').first().data('index') > 0) {
        $('.grid-content-loading-start').removeClass('hidden');
    };
    if ($('.grid-item').last().data('index') < $('.grid-item').last().data('count') - 1) {
        $('.grid-content-loading-end').removeClass('hidden');
    }
    window.scrollTo(0, savedScroll);
    setTimeout(function () {
        scrollToPhoto($(gridItem));
        connectGridLoaderObservers();
        mouseEnterEnabled = true;
    }, 100);
}

function isLoupeOpen() {
    return $('.container').hasClass('show-loupe');
}

function loupePrev() {
    let prev = $(loupeElement).parent().prev();
    if (prev.length > 0) {
        setLoupePhoto(prev);
        selectPhoto(prev);
        if (prev.data('index') >= 1 && $('[data-index="' + (prev.data('index') - 1) + '"]').length == 0) {
            loadGrid(true);
        }
    }
}

function loupeNext(loop=false) {
    let next = $(loupeElement).parent().next();
    if (next.length > 0) {
        setLoupePhoto(next);
        selectPhoto(next);
        if (next.data('index') < $(loupeElement).parent().data('count') - 1 && $('[data-index="' + (next.data('index') + 1) + '"]').length == 0) {
            loadGrid();
        }
    } else if (loop) {
        loupeFirst();
    }
}

function loupeFirst() {
    let first = $(loupeElement).parents('.grid-content').children().first();
    setLoupePhoto(first);
    selectPhoto(first);
}

function loupeLast() {
    let last = $(loupeElement).parents('.grid-content').children().last();
    setLoupePhoto(last);
    selectPhoto(last);
}

function toggleShowMetadata() {
    showMetadata = !showMetadata;
    if (showMetadata) {
        $('.loupe-metadata').removeClass('invisible');
    } else {
        $('.loupe-metadata').addClass('invisible');
    }
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

function downloadCurrentPhoto() {
    window.open($(loupeElement).data('src-download'));
}


$(function() {
    let preselectedUID = undefined;
    if (window.location.hash) {
        if (window.location.hash == '#nav') {
            openNavigationPanel();
        } else {
            let hashValue = window.location.hash.substr(1);
            if (hashValue.length == UID_LENGTH) {
                let allowedChars = UID_CHARS.split('');
                preselectedUID = hashValue.split('').filter(c => allowedChars.indexOf(c) >= 0).join('');
            }
        }
    }

    loadGrid(false, preselectedUID);

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
        toggleShowMetadata();
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
    $('.grid-action-open-navigation-panel').on('click', function(event) {
        openNavigationPanel();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.navigation-panel-close').on('click', function(event) {
        closeNavigationPanel();
        event.preventDefault();
        event.stopPropagation();
    });
    $('.navigation-panel-background').on('click', function(event) {
        closeNavigationPanel();
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
        if (event.ctrlKey || event.shiftKey || event.metaKey || event.altKey) {
            return;
        }
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

        } else if (event.code == 'KeyI') {
            event.preventDefault();
            toggleShowMetadata();

        } else if (event.code == 'KeyD') {
            event.preventDefault();
            if (isLoupeOpen()) {
                downloadCurrentPhoto();
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
            if (!isLoupeOpen()) {
                gridZoomIn();
            }

        } else if (event.key == "-") {
            event.preventDefault();
            if (!isLoupeOpen()) {
                gridZoomOut();
            }
        }
    };
});