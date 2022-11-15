let savedScroll = 0;
let loupeElement = undefined;
let opacityTransitionInProgress = false;
let opacityTransitionTimeout = undefined;
let slideshowIntervalTimer = undefined;
let loadGridBatchSize = 50;
let mouseEnterEnabled = true;
let loadGridRequest = undefined;
let loadNavRequest = undefined;
let slideshowTimeStep = 20;
let slideshowTimeCounter = 0;

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

function loadFolder(url, gridURL, navURL, addToHistory=true) {
    stopSlideshow();
    disconnectGridLoaderObservers();
    if (gridURL == loadGridURL) {
        return;
    }
    if (loadGridRequest) {
        loadGridRequest.abort();
        loadGridRequest = undefined
    }
    if (loadNavRequest) {
        loadNavRequest.abort();
        loadNavRequest = undefined
    }
    if (url == '/') {
        document.title = TITLE;
    } else {
        let urlSplit = url.split('/').filter(e => e != '');
        document.title = decodeURI(urlSplit[urlSplit.length-1]) + ' - ' + TITLE;
    }
    loadGridURL = gridURL;
    loadNavURL = navURL;
    if (addToHistory) {
        url += window.location.hash;
        history.pushState({'url': url, 'gridURL': gridURL, 'navURL': navURL, 'navPanelOpen': isNavigationPanelOpen()}, '', url);
    }
    $('.grid-content').empty();
    loadNav();
    loadGrid();
}

function loadNav() {
    $('.nav-loading').removeClass('hidden');
    if (loadNavRequest) {
        loadNavRequest.abort();
        loadNavRequest = undefined;
    }
    loadNavRequest = new XMLHttpRequest();
    loadNavRequest.onreadystatechange = function() {
        if (this.readyState == 4) {
            if (this.status == 200) {
                $('.nav-loading').addClass('hidden');
                $('.navigation-panel-content').replaceWith(loadNavRequest.responseText);
                $('.nav-link').on('click', function(event) {
                    let link = event.target;
                    if (link.nodeName != 'A') {
                        link = $(link).parents('.nav-link');
                    }
                    loadFolder($(link).attr('href'), $(link).data('load-url'), $(link).data('nav-url'));
                    event.preventDefault();
                    event.stopPropagation();
                });
                $('.navigation-panel-subdir').on('mouseenter', function(event) {
                    let subdir = event.target;
                    if (!$(event.target).hasClass('navigation-panel-subdir')) {
                        subdir = $(event.target).parents('.navigation-panel-subdir');
                    }
                    $('.navigation-panel-subdir.selected').removeClass('selected');
                    $(subdir).addClass('selected');
                });
                let parentLinkWrapper = $('.navigation-panel-subdir-parent');
                let buttonNavigateUp = $('.grid-action-navigate-up');
                if (parentLinkWrapper.length > 0) {
                    let parentLink = parentLinkWrapper.children('.nav-link');
                    let buttonNavigateUpLink = buttonNavigateUp.children('.nav-link');
                    buttonNavigateUpLink.attr('href', parentLink.attr('href'));
                    buttonNavigateUpLink.data('load-url', parentLink.data('load-url'));
                    buttonNavigateUpLink.data('nav-url', parentLink.data('nav-url'));
                    buttonNavigateUp.removeClass('hidden');
                } else {
                    buttonNavigateUp.addClass('hidden');
                }
                updateNPhotos();
                loadNavRequest = undefined;
            } else {
                $('.nav-loading').addClass('hidden');
                $('.nav-loading-error').removeClass('hidden');
            }
        }
    };
    loadNavRequest.open('GET', loadNavURL, true);
    loadNavRequest.send();
    disconnectGridLoaderObservers();
}

function isLoadingNav() {
    return loadNavRequest != undefined;
}

function loadGrid(before=false, preselectedUID=undefined, around=false) {
    if (loadGridRequest) {
        loadGridRequest.abort();
        loadGridRequest = undefined;
    }
    loadGridRequest = new XMLHttpRequest();
    loadGridRequest.onreadystatechange = function() {
        if (this.readyState == 4) {
            if (this.status == 200) {
                $('.grid-loading').addClass('hidden');
                let gridContent = $('.grid-content');
                let response = $(loadGridRequest.responseText.replace(/\n/g, '').trim());
                if (response.length > 0) {
                    response.each(function(index, gridItem) {
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
                    $('.background').addClass('hidden');
                } else {
                    $('.background').removeClass('hidden');
                }
                loadGridRequest = undefined;
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
                $('.grid-actions-topright').removeClass('hidden');
                updateNPhotos();
            } else {
                $('.grid-loading').addClass('hidden');
                $('.grid-loading-error').removeClass('hidden');
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
    loadGridRequest.open('GET', loadGridURL + args, true);
    loadGridRequest.send();
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
            if (this.readyState == 4) {
                if (this.status == 200) {
                    if ($(gridItem).children('img').length > 0) {
                        return;
                    }
                    $(request.responseText.replace(/\n/g, '').trim()).prependTo($(gridItem));
                    let image = $(gridItem).children('.photo');
                    $(image).on('load', function() {
                        $(image).parent().children('.loading').remove();
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
                } else {
                    $(gridItem).children('.loading').remove();
                    $(gridItem).children('.loading-error').removeClass('hidden');
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

function isGridSelected() {
    return $('.grid-item.selected').length > 0;
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

function openNavigationPanel(addToHistory=true) {
    $('.navigation-panel-container').removeClass('invisible');
    $('body').addClass('no-scroll');
    if (addToHistory) {
        history.pushState({'url': window.location.pathname, 'gridURL': loadGridURL, 'navURL': loadNavURL, 'navPanelOpen': true}, '', window.location.pathname + '#nav');
    }
}

function closeNavigationPanel(addToHistory=true) {
    $('.navigation-panel-container').addClass('invisible');
    $('body').removeClass('no-scroll');
    if (addToHistory) {
        history.pushState({'url': window.location.pathname, 'gridURL': loadGridURL, 'navURL': loadNavURL, 'navPanelOpen': false}, '', window.location.pathname);
    }
}

function toggleNavigationPanel() {
    if (isNavigationPanelOpen()) {
        closeNavigationPanel();
    } else {
        openNavigationPanel();
    }
}

function isNavigationPanelOpen() {
    return !$('.navigation-panel-container').hasClass('invisible');
}

function navigationPanelPrev() {
    let selected = $('.navigation-panel-subdir.selected');
    if (selected.length == 0) {
        $('.navigation-panel-subdir').last().addClass('selected');
    } else {
        let prev = $(selected).prev();
        if (prev.length > 0 && $(prev).hasClass('navigation-panel-subdir')) {
            $(selected.removeClass('selected'));
            $(prev).addClass('selected');
        }
    }
}

function navigationPanelNext() {
    let selected = $('.navigation-panel-subdir.selected');
    if (selected.length == 0) {
        $('.navigation-panel-subdir').first().addClass('selected');
    } else {
        let next = $(selected).next();
        if (next.length > 0) {
            $(selected.removeClass('selected'));
            $(next).addClass('selected');
        }
    }
}

function navigationPanelFirst() {
    $('.navigation-panel-subdir.selected').removeClass('selected');
    $('.navigation-panel-subdir').first().addClass('selected');
}

function navigationPanelLast() {
    $('.navigation-panel-subdir.selected').removeClass('selected');
    $('.navigation-panel-subdir').last().addClass('selected');
}

function scrollNavigationToSelectedLink() {
    let panel = $('.navigation-panel-subdirs');
    let navLink = $('.navigation-panel-subdir.selected');
    if (navLink.length > 0) {
        let panelTop = panel.offset().top;
        let panelBottom = panelTop + panel.height();
        let navLinkTop = navLink.offset().top;
        let navLinkBottom = navLinkTop + navLink.outerHeight();
        if (navLinkTop < panelTop) {
            panel.get(0).scrollBy(0, navLinkTop - panelTop);
        } else if (navLinkBottom > panelBottom) {
            panel.get(0).scrollBy(0, navLinkBottom - panelBottom);
        }
    }
}

function navigateUp() {
    let buttonNavigateUp = $('.grid-action-navigate-up');
    if (!buttonNavigateUp.hasClass('hidden')) {
        let buttonNavigateUpLink = buttonNavigateUp.children('.nav-link');
        loadFolder(buttonNavigateUpLink.attr('href'), buttonNavigateUpLink.data('load-url'), buttonNavigateUpLink.data('nav-url'));
    }
}

function loadSelectedNavigationLink() {
    let link = $('.navigation-panel-subdir.selected .nav-link');
    if (link.length > 0) {
        loadFolder($(link).attr('href'), $(link).data('load-url'), $(link).data('nav-url'));
    }
}

function updateNPhotos() {
    let span = $('.navigation-panel-n-photos-value');
    if (span.length > 0) {
        let gridItem = $('.grid-item').first();
        if (gridItem.length > 0) {
            span.text(gridItem.data('count'));
            span.parent().removeClass('invisible');
        }
    }
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
    slideshowTimeCounter = 0;
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
    slideshowTimeCounter = 0;
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
    slideshowTimeCounter = 0;
    let first = $(loupeElement).parents('.grid-content').children().first();
    setLoupePhoto(first);
    selectPhoto(first);
}

function loupeLast() {
    slideshowTimeCounter = 0;
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
        let btnStop = $('.loupe-action-slideshow-stop');
        btnStop.addClass('button-progress');
        btnStop.removeClass('hidden');
        slideshowIntervalTimer = setInterval(function() {
            slideshowTimeCounter++;
            if (slideshowTimeCounter * slideshowTimeStep >= slideshowDelay) {
                slideshowTimeCounter = 0;
                loupeNext(true);
            }
            updateSlideshowProgress(100.0 * slideshowTimeCounter * slideshowTimeStep / slideshowDelay);
        }, slideshowTimeStep);
    }
}

function stopSlideshow() {
    if (slideshowIntervalTimer) {
        $('.loupe-action-slideshow-start').removeClass('hidden');
        let btnStop = $('.loupe-action-slideshow-stop');
        btnStop.addClass('hidden');
        btnStop.removeClass('button-progress');
        updateSlideshowProgress(0);
        clearInterval(slideshowIntervalTimer);
        slideshowIntervalTimer = undefined;
        slideshowTimeCounter = 0;
    }
}

function updateSlideshowProgress(progress) {
    if (progress > 100) {
        progress = 100;
    }
    $('.loupe-action-slideshow-stop').css('--progress', progress + 'deg');
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
        let hashValue = window.location.hash.substr(1);
        if (hashValue.length == UID_LENGTH) {
            let allowedChars = UID_CHARS.split('');
            preselectedUID = hashValue.split('').filter(c => allowedChars.indexOf(c) >= 0).join('');
        }
    }
    if (preselectedUID == undefined && (window.location.hash == '#nav' || openNav)) {
        openNavigationPanel();
        openNav = false;
    }

    loadNav();
    loadGrid(false, preselectedUID);

    $(window).on('popstate', function(event) {
        if (event.state && 'url' in event.state && 'gridURL' in event.state && 'navURL' in event.state) {
            loadFolder(event.state.url, event.state.gridURL, event.state.navURL, false);
            if (event.state.navPanelOpen && !isNavigationPanelOpen()) {
                openNavigationPanel(false);
            } else if (!event.state.navPanelOpen && isNavigationPanelOpen()) {
                closeNavigationPanel(false);
            }
        }
    });

    $('.grid-action-navigate-up .nav-link').on('click', function(event) {
        navigateUp();
        event.preventDefault();
        event.stopPropagation();
    });
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
        if (event.code == 'Escape' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                closeLoupe();
            } else if (isNavigationPanelOpen()) {
                closeNavigationPanel();
            } else if (isGridSelected()) {
                deselect();
            } else {
                openNavigationPanel();
            }

        } else if (event.code == 'ArrowLeft' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupePrev();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    navigateUp();
                }
            } else {
                selectPrev();
            }

        } else if (event.code == 'ArrowRight' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    loadSelectedNavigationLink();
                }
            } else {
                selectNext();
            }

        } else if (event.code == 'ArrowDown' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    navigationPanelNext();
                    scrollNavigationToSelectedLink();
                }
            } else {
                selectBelow();
            }

        } else if (event.code == 'ArrowUp' && !event.ctrlKey && !event.shiftKey && !event.metaKey) {
            event.preventDefault();
            if (event.altKey) {
                navigateUp();
            } else if (isLoupeOpen()) {
                loupePrev();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    navigationPanelPrev();
                    scrollNavigationToSelectedLink();
                }
            } else {
                selectAbove();
            }

        } else if (event.code == 'Home' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeFirst();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    navigationPanelFirst();
                }
            } else {
                selectFirst();
            }

        } else if (event.code == 'End' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeLast();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    navigationPanelLast();
                }
            } else {
                selectLast();
            }

        } else if ((event.code == 'Enter' || event.code == 'KeyF') && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                loupeNext();
            } else if (isNavigationPanelOpen()) {
                if (!isLoadingNav()) {
                    loadSelectedNavigationLink();
                }
            } else {
                let selected = $('.grid-item.selected');
                if (selected.length == 0) {
                    selectFirst();
                }
                openLoupe($('.grid-item.selected'));
            }

        } else if (event.code == 'KeyN' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            toggleNavigationPanel();

        } else if (event.code == 'KeyI' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            toggleShowMetadata();

        } else if (event.code == 'KeyD' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (isLoupeOpen()) {
                downloadCurrentPhoto();
            }

        } else if (event.code == 'Space' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
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

        } else if (event.key == '+' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (!isLoupeOpen()) {
                gridZoomIn();
            }

        } else if (event.key == '-' && !event.ctrlKey && !event.shiftKey && !event.metaKey && !event.altKey) {
            event.preventDefault();
            if (!isLoupeOpen()) {
                gridZoomOut();
            }
        }
    };
});