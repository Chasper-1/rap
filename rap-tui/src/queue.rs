/// Логика очереди воспроизведения в пределах текущей папки.
/// Без цикла: после последнего файла воспроизведение останавливается.

/// Возвращает индекс следующего файла, либо `None`, если текущий — последний.
pub fn next_index(len: usize, current: usize) -> Option<usize> {
    let next = current + 1;
    if next < len { Some(next) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_in_middle() {
        assert_eq!(next_index(5, 1), Some(2));
    }

    #[test]
    fn next_from_last_is_none() {
        assert_eq!(next_index(5, 4), None);
    }

    #[test]
    fn next_in_single_file() {
        assert_eq!(next_index(1, 0), None);
    }

    #[test]
    fn next_in_empty_list() {
        assert_eq!(next_index(0, 0), None);
    }
}
