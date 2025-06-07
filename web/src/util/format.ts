// Comprehensive formatting utilities

// Date/Time formatting
export const formatDate = (dateString: string) => {
    const date = new Date(dateString);

    return date.toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
    });
};

export const formatRelativeTime = (dateString: string) => {
    const date = new Date(dateString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) return 'today';

    if (diffDays === 1) return '1 day ago';

    if (diffDays < 30) return `${diffDays} days ago`;

    if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;

    return `${Math.floor(diffDays / 365)} years ago`;
};

// Number formatting
export const formatCompact = (num: number) => {
    if (num >= 1000) {
        return `${(num / 1000).toFixed(1)}k`;
    }

    return num.toString();
};

export const formatBigNumber = (num: number) => {
    if (num > 1000000) {
        return (num / 1000000).toFixed(1) + 'M';
    }

    if (num > 1000) {
        return (num / 1000).toFixed(1) + 'K';
    }

    return num;
};

// Text processing
export const truncateText = (text: string, maxLength: number = 200) => {
    if (text.length <= maxLength) return text;

    return text.slice(0, maxLength) + '...';
};

export const getPlainText = (html: string) => {
    const tempDiv = document.createElement('div');

    tempDiv.innerHTML = html;

    return tempDiv.textContent || tempDiv.innerText || '';
};

// JSON utilities
export const isValidJSON = (str: string): boolean => {
    try {
        JSON.parse(str);

        return true;
    } catch {
        return false;
    }
};

export const formatJSON = (str: string): string => {
    try {
        return JSON.stringify(JSON.parse(str), null, 2);
    } catch {
        return str;
    }
};

// Tool name formatting
export const getToolDisplayName = (toolName: string) => {
    // Convert snake_case to readable format
    return toolName
        .split('_')
        .map((word, i) => (i !== 0 ? word : word.charAt(0).toUpperCase() + word.slice(1)))
        .join(' ');
};

// Pluralization utility
export const pluralize = (count: number, singular: string, plural?: string) => {
    if (count === 1) return singular;

    return plural || `${singular}s`;
};

// Results count message formatting
export const getResultsCountMessage = (toolName: string, topics: number, posts: number) => {
    const totalHits = topics + posts;

    switch (toolName) {
        case 'search_topics':
            return `${topics} ${pluralize(topics, 'topic')} found`;
        case 'search_posts':
            return `${posts} ${pluralize(posts, 'post')} found`;
        case 'search_forum':
        default:
            return `${totalHits} ${pluralize(totalHits, 'result')} found`;
    }
};
