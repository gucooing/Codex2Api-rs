//! Checked decoding of PostgreSQL signed integers into public unsigned limits.

#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub(crate) struct OptionalU32(Option<i64>);

impl TryFrom<OptionalU32> for Option<u32> {
    type Error = std::num::TryFromIntError;

    fn try_from(value: OptionalU32) -> Result<Self, Self::Error> {
        value.0.map(u32::try_from).transpose()
    }
}
