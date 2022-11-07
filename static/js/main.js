$(document).ready(function() {
    $('.grid .grid-item .photo').on('click', function() {
        $('.photo-large').attr('src', $(this).attr('src'));
        $('.container').addClass('show-loupe');
    });

    $('.loupe').on('click', function() {
        $('.container').removeClass('show-loupe');
    });
});