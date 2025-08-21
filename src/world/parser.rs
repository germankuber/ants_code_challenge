use crate::direction::Direction;
use crate::error::{ParseError, Result};
use crate::world::node::Node;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Parse a world from a file path
pub fn parse_world(path: &str) -> Result<(Vec<String>, Vec<Node>)> {
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(64 * 1024, file);

    let mut names: Vec<String> = Vec::with_capacity(1024);
    let mut name_to_id: HashMap<String, u32> = HashMap::with_capacity(1024);
    let mut edges: Vec<(u32, Direction, String)> = Vec::with_capacity(4096);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let colony = parts
            .next()
            .ok_or_else(|| ParseError::InvalidLine("missing colony name".to_string()))?;

        let src_id = *name_to_id.entry(colony.to_string()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(colony.to_string());
            id
        });

        for kv in parts {
            if let Some(eq) = kv.find('=') {
                let dir_s = &kv[..eq];
                let dst_s = &kv[eq + 1..];
                let dir: Direction = dir_s.parse()?;
                edges.push((src_id, dir, dst_s.to_string()));
            }
        }
    }

    // Ensure ids exist for destinations not seen as sources
    for (_, _, dst) in &edges {
        name_to_id.entry(dst.clone()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(dst.clone());
            id
        });
    }

    let mut nodes: Vec<Node> = (0..names.len()).map(|i| Node::new(i as u32)).collect();

    for (src, dir, dst_name) in &edges {
        if let Some(&dst) = name_to_id.get(dst_name) {
            nodes[*src as usize].set_neighbor(dir.index(), dst);
        }
    }

    Ok((names, nodes))
}

/// Parse a world directly from an in-memory string for testing
pub fn parse_world_from_str(src: &str) -> (Vec<String>, Vec<Node>) {
    let mut names: Vec<String> = Vec::new();
    let mut name_to_id: HashMap<String, u32> = HashMap::new();
    let mut edges: Vec<(u32, Direction, String)> = Vec::new();

    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let colony = parts.next().expect("missing colony name");

        let src_id = *name_to_id.entry(colony.to_string()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(colony.to_string());
            id
        });

        for kv in parts {
            if let Some(eq) = kv.find('=') {
                let dir_s = &kv[..eq];
                let dst_s = &kv[eq + 1..];
                let dir = dir_s.parse().expect("invalid direction");
                edges.push((src_id, dir, dst_s.to_string()));
            }
        }
    }

    for (_, _, dst) in &edges {
        name_to_id.entry(dst.clone()).or_insert_with(|| {
            let id = names.len() as u32;
            names.push(dst.clone());
            id
        });
    }

    let mut nodes: Vec<Node> = (0..names.len()).map(|i| Node::new(i as u32)).collect();
    for (src, dir, dst_name) in &edges {
        let dst = *name_to_id.get(dst_name).unwrap();
        nodes[*src as usize].set_neighbor(dir.index(), dst);
    }

    (names, nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_map() {
        let src = "A north=B\nB south=A\n";
        let (names, nodes) = parse_world_from_str(src);
        
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"A".to_string()));
        assert!(names.contains(&"B".to_string()));
        
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_parse_empty_lines() {
        let src = "A north=B\n\nB south=A\n";
        let (names, _) = parse_world_from_str(src);
        
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn test_parse_multiple_directions() {
        let src = "A north=B east=C west=D\n";
        let (names, nodes) = parse_world_from_str(src);
        
        assert_eq!(names.len(), 4);
        
        let a_idx = names.iter().position(|n| n == "A").unwrap();
        let b_idx = names.iter().position(|n| n == "B").unwrap();
        let c_idx = names.iter().position(|n| n == "C").unwrap();
        let d_idx = names.iter().position(|n| n == "D").unwrap();
        
        assert_eq!(nodes[a_idx].get_neighbor(Direction::North.index()), Some(b_idx as u32));
        assert_eq!(nodes[a_idx].get_neighbor(Direction::East.index()), Some(c_idx as u32));
        assert_eq!(nodes[a_idx].get_neighbor(Direction::West.index()), Some(d_idx as u32));
    }
}
