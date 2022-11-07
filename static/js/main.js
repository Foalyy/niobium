$(document).ready(function() {
    $('.grid .grid-item .photo').on('click', function() {
        $('.photo-large').attr('src', $(this).data('src'));
        $('.container').addClass('show-loupe');
    });
    $('.grid .grid-item .photo').each(function(element, ) {
        $(element).attr('src', $(element).data('thumbnail'));
    });

    $('.loupe').on('click', function() {
        $('.container').removeClass('show-loupe');
    });
});