"""Hypothesis strategies for the teams domain.

Provides reusable property-based testing strategies for generating valid and
invalid inputs across team, user, invitation, and API key operations.
"""

from hypothesis import HealthCheck, settings
from hypothesis import strategies as st

# ---------------------------------------------------------------------------
# E2E-friendly Hypothesis settings profile
# ---------------------------------------------------------------------------
e2e_settings = settings(
    max_examples=30,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)

# ---------------------------------------------------------------------------
# Team data
# ---------------------------------------------------------------------------
team_names = (
    st.text(
        alphabet=st.characters(
            min_codepoint=32,
            max_codepoint=126,
            blacklist_categories=("Cc", "Cs"),
        ),
        min_size=1,
        max_size=100,
    )
    .filter(lambda s: not s.isspace())
    .filter(lambda s: s.strip() == s)
)

team_slugs = (
    st.text(
        alphabet="abcdefghijklmnopqrstuvwxyz0123456789-",  # pragma: allowlist secret
        min_size=1,
        max_size=50,
    )
    .filter(lambda s: not s.startswith("-") and not s.endswith("-"))
    .filter(lambda s: "--" not in s)
)

invalid_team_names = st.one_of(
    st.just(""),
    st.text(min_size=101, max_size=200),
)

invalid_team_slugs = st.one_of(
    st.text(alphabet="-", min_size=1, max_size=5),
    st.text(
        alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        min_size=1,
        max_size=10,
    ),
    st.just("-leading"),
    st.just("trailing-"),
)

# ---------------------------------------------------------------------------
# User data
# ---------------------------------------------------------------------------
user_names = (
    st.text(
        alphabet=st.characters(
            min_codepoint=32,
            max_codepoint=126,
            blacklist_categories=("Cc", "Cs"),
        ),
        min_size=1,
        max_size=100,
    )
    .filter(lambda s: not s.isspace())
    .filter(lambda s: s.strip() == s)
)

avatar_urls = st.from_regex(
    r"https://example\.com/avatars/[a-z0-9]{8}\.png",
    fullmatch=True,
)

# ---------------------------------------------------------------------------
# Roles & Tiers
# ---------------------------------------------------------------------------
membership_roles = st.sampled_from(["owner", "admin", "member", "viewer"])
invitation_roles = st.sampled_from(["admin", "member", "viewer"])  # no 'owner' (INV-I2)
user_tiers = st.sampled_from(["starter", "creator"])

# ---------------------------------------------------------------------------
# API Key data
# ---------------------------------------------------------------------------
api_key_names = st.text(min_size=1, max_size=100).filter(lambda s: s.strip() == s)

starter_scopes = st.lists(
    st.sampled_from(
        ["generate", "jobs:read", "jobs:write", "assets:read", "assets:write"]
    ),
    min_size=1,
    max_size=5,
    unique=True,
)

creator_scopes = st.lists(
    st.sampled_from(
        [
            "generate",
            "jobs:read",
            "jobs:write",
            "assets:read",
            "assets:write",
            "projects:read",
            "projects:write",
            "team:read",
            "team:admin",
            "*",
        ]
    ),
    min_size=1,
    max_size=10,
    unique=True,
)

_VALID_SCOPES = frozenset(
    {
        "generate",
        "jobs:read",
        "jobs:write",
        "assets:read",
        "assets:write",
        "projects:read",
        "projects:write",
        "team:read",
        "team:admin",
        "*",
    }
)

invalid_scopes = st.lists(
    st.text(alphabet="abcdefghijklmnopqrstuvwxyz_:", min_size=3, max_size=20).filter(
        lambda s: s not in _VALID_SCOPES
    ),
    min_size=1,
    max_size=3,
)

# ---------------------------------------------------------------------------
# Credits
# ---------------------------------------------------------------------------
valid_credits = st.integers(min_value=0, max_value=1_000_000)
