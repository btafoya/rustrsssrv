/* global jQuery */
(function ($) {
    'use strict';

    let selectAllMatching = false;

    function selectedIds() {
        return $('.article-select:checked')
            .map(function () {
                return parseInt($(this).data('article-id'), 10);
            })
            .get();
    }

    function currentFilter() {
        const $bar = $('#bulk-actions-bar');
        const feedId = $bar.data('feed-id');
        const filter = $bar.data('filter');
        let isRead = null;
        let isStarred = null;
        if (filter === 'unread') {
            isRead = false;
        } else if (filter === 'read') {
            isRead = true;
        } else if (filter === 'starred') {
            isStarred = true;
        }
        return {
            feed_id: feedId === '' || feedId === undefined ? null : feedId,
            is_read: isRead,
            is_starred: isStarred
        };
    }

    function resetSelection() {
        $('.article-select, #select-all-page').prop('checked', false);
        selectAllMatching = false;
        $('#select-all-matching').addClass('hidden').text('Select all matching this filter');
    }

    function removeCard($checkbox) {
        $checkbox.closest('.rounded-lg').fadeOut(150, function () {
            $(this).remove();
        });
    }

    function updateReadStyling($checkbox, isRead) {
        $checkbox.closest('.rounded-lg').find('h2').toggleClass('text-blue-700', !isRead);
    }

    function updateStarStyling($checkbox, starred) {
        const $btn = $checkbox.closest('.rounded-lg').find('.star-toggle');
        $btn.data('starred', starred);
        $btn.attr('data-starred', starred);
        $btn.toggleClass('text-orange-500', starred).toggleClass('text-gray-400', !starred);
    }

    $(document).on('change', '#select-all-page', function () {
        $('.article-select').prop('checked', $(this).is(':checked'));
        selectAllMatching = false;
        $('#select-all-matching').toggleClass('hidden', !$(this).is(':checked'));
    });

    $(document).on('change', '.article-select', function () {
        selectAllMatching = false;
        if (!$(this).is(':checked')) {
            $('#select-all-page').prop('checked', false);
            $('#select-all-matching').addClass('hidden');
        }
    });

    $(document).on('click', '#select-all-matching', function () {
        selectAllMatching = true;
        $(this).text('All matching articles selected');
    });

    $(document).on('click', '#bulk-apply', function () {
        const action = $('#bulk-action').val();
        const ids = selectedIds();
        if (!selectAllMatching && ids.length === 0) {
            return;
        }

        const label = selectAllMatching ? 'all matching' : ids.length;
        if (action === 'hide' && !window.confirm('Hide ' + label + ' article(s)? This cannot be undone.')) {
            return;
        }

        const payload = { action: action };
        if (selectAllMatching) {
            payload.filter = currentFilter();
        } else {
            payload.article_ids = ids;
        }

        $.ajax({
            url: '/api/v1/articles/bulk',
            method: 'POST',
            contentType: 'application/json',
            data: JSON.stringify(payload),
            success: function () {
                if (selectAllMatching) {
                    window.location.reload();
                    return;
                }
                $('.article-select:checked').each(function () {
                    const $checkbox = $(this);
                    if (action === 'hide') {
                        removeCard($checkbox);
                    } else if (action === 'read') {
                        updateReadStyling($checkbox, true);
                    } else if (action === 'unread') {
                        updateReadStyling($checkbox, false);
                    } else if (action === 'star') {
                        updateStarStyling($checkbox, true);
                    } else if (action === 'unstar') {
                        updateStarStyling($checkbox, false);
                    }
                });
                resetSelection();
            },
            error: function (xhr) {
                console.error('Bulk action failed', xhr.status, xhr.responseText);
            }
        });
    });
})(jQuery);
