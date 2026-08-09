/* global jQuery */
(function ($) {
    'use strict';

    const emptyStar = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-6 h-6"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>';
    const filledStar = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-6 h-6"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>';

    function isStarred($btn) {
        return $btn.data('starred') === true || $btn.data('starred') === 'true';
    }

    function renderStar($btn, starred) {
        $btn.data('starred', starred);
        $btn.attr('data-starred', starred);
        if (starred) {
            $btn.addClass('text-orange-500').removeClass('text-gray-400');
            $btn.html(filledStar);
        } else {
            $btn.addClass('text-gray-400').removeClass('text-orange-500');
            $btn.html(emptyStar);
        }
    }

    $(document).on('click', '.star-toggle', function (e) {
        e.preventDefault();
        e.stopPropagation();

        const $btn = $(this);
        const id = $btn.data('article-id');
        const currentlyStarred = isStarred($btn);
        const action = currentlyStarred ? 'unstar' : 'star';
        const url = '/api/v1/articles/' + id + '/' + action;

        $.ajax({
            url: url,
            method: 'POST',
            success: function () {
                renderStar($btn, !currentlyStarred);
            },
            error: function (xhr) {
                console.error('Failed to toggle star for article', id, xhr.status, xhr.responseText);
            }
        });
    });
})(jQuery);
